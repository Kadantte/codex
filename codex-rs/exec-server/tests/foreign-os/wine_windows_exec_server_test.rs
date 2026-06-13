#[cfg(not(target_os = "linux"))]
compile_error!("the Wine exec-server test can only run on Linux");

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;
use app_test_support::TestAppServer;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_sequence;
use app_test_support::to_response;
use app_test_support::write_mock_responses_config_toml;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SandboxMode;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnEnvironmentParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use codex_exec_server::CODEX_EXEC_SERVER_URL_ENV_VAR;
use codex_exec_server::REMOTE_ENVIRONMENT_ID;
use codex_features::Feature;
use codex_utils_path_uri::NativePathString;
use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::ChildStdout;
use wine_test_support::WineTestCommand;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_preserves_windows_environment_context_under_wine() -> Result<()> {
    let executable = codex_utils_cargo_bin::cargo_bin("wine-windows-exec-server")?;
    let mut server = WineTestCommand::new(executable)
        .env("CODEX_HOME", r"C:\codex-home")
        .spawn()?;
    let stdout = server.take_stdout();

    server.scope(exercise_through_app_server(stdout)).await
}

async fn exercise_through_app_server(stdout: ChildStdout) -> Result<()> {
    let mut lines = BufReader::new(stdout).lines();
    let websocket_url = loop {
        let line = lines
            .next_line()
            .await?
            .context("Wine exec-server exited before reporting its URL")?;
        if line.starts_with("ws://") {
            break line;
        }
    };

    let responses_server = create_mock_responses_server_sequence(vec![
        create_final_assistant_message_sse_response("done")?,
    ])
    .await;
    let codex_home = TempDir::new()?;
    write_mock_responses_config_toml(
        codex_home.path(),
        &responses_server.uri(),
        &BTreeMap::from([(Feature::UnifiedExec, true)]),
        /*auto_compact_limit*/ 1_000_000,
        /*requires_openai_auth*/ None,
        "mock_provider",
        "compact",
    )?;

    let app_server_program =
        codex_utils_cargo_bin::find_resource!("../../../app-server/codex-app-server")?;
    let mut app_server = TestAppServer::new_with_program_and_env(
        codex_home.path(),
        &app_server_program,
        &[(CODEX_EXEC_SERVER_URL_ENV_VAR, Some(&websocket_url))],
    )
    .await?;
    app_server.initialize().await?;

    let remote_environment = TurnEnvironmentParams {
        environment_id: REMOTE_ENVIRONMENT_ID.to_string(),
        cwd: NativePathString::new(r"C:\workspace"),
    };
    let thread_request_id = app_server
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            sandbox: Some(SandboxMode::DangerFullAccess),
            environments: Some(vec![remote_environment]),
            ..Default::default()
        })
        .await?;
    let thread_response: JSONRPCResponse = app_server
        .read_stream_until_response_message(RequestId::Integer(thread_request_id))
        .await?;
    let ThreadStartResponse { thread, .. } = to_response(thread_response)?;

    let turn_request_id = app_server
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id,
            input: vec![UserInput::Text {
                text: "run the Windows smoke command".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_response: JSONRPCResponse = app_server
        .read_stream_until_response_message(RequestId::Integer(turn_request_id))
        .await?;
    let TurnStartResponse { turn } = to_response(turn_response)?;

    let completed_notification = app_server
        .read_stream_until_notification_message("turn/completed")
        .await?;
    let completed: TurnCompletedNotification = serde_json::from_value(
        completed_notification
            .params
            .context("turn/completed notification should include params")?,
    )?;
    assert_eq!(completed.turn.id, turn.id);
    assert_eq!(completed.turn.status, TurnStatus::Completed);

    let requests = responses_server
        .received_requests()
        .await
        .context("failed to fetch received requests")?;
    let model_request = requests
        .iter()
        .find(|request| request.url.path().ends_with("/responses"))
        .context("expected model request")?;
    let model_request_body = model_request
        .body_json::<Value>()
        .context("model request body should be JSON")?;
    let environment_context = model_request_body["input"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.get("role").and_then(Value::as_str) == Some("user"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|span| span.get("type").and_then(Value::as_str) == Some("input_text"))
        .filter_map(|span| span.get("text").and_then(Value::as_str))
        .find(|text| text.starts_with("<environment_context>"))
        .context("environment context should be model visible")?;
    assert!(environment_context.contains(r"<cwd>C:\workspace</cwd>"));
    assert!(
        environment_context.contains("<shell>cmd</shell>")
            || environment_context.contains("<shell>powershell</shell>"),
        "unexpected Windows shell context: {environment_context}"
    );
    assert!(!environment_context.contains("/C:/workspace"));
    assert!(!environment_context.contains("bash"));
    assert!(!environment_context.contains("/bin/"));

    Ok(())
}
