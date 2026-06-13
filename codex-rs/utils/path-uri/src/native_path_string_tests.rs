use super::*;
use crate::PathUri;
use pretty_assertions::assert_eq;

#[test]
fn renders_posix_paths_on_every_host() {
    for (uri, expected) in [
        ("file:///", "/"),
        ("file:///home/alice/a%20file.rs", "/home/alice/a file.rs"),
        ("file:///tmp/", "/tmp/"),
        ("file:///C:/Project", "/C:/Project"),
        ("file:///tmp/%E2%98%83", "/tmp/☃"),
        ("file:///tmp/a%5Cb", "/tmp/a\\b"),
    ] {
        let path = PathUri::parse(uri).expect("valid file URI");
        assert_eq!(
            NativePathString::from_path_uri(&path, PathConvention::Posix)
                .map(NativePathString::into_string),
            Ok(expected.to_string()),
            "rendering {uri}"
        );
    }
}

#[test]
fn renders_windows_drive_paths_on_every_host() {
    for (uri, expected) in [
        (
            "file:///C:/Users/Alice%20Smith/src/main.rs",
            r"C:\Users\Alice Smith\src\main.rs",
        ),
        ("file:///C:/", "C:\\"),
        ("file:///C:", "C:\\"),
        ("file:///d:/snowman/%E2%98%83", r"d:\snowman\☃"),
        ("file:///C:/tmp/", "C:\\tmp\\"),
        ("file:///C:/test%20with%20%25/path", r"C:\test with %\path"),
        (
            "file:///C:/test%20with%20%2525/c%23code",
            r"C:\test with %25\c#code",
        ),
        (
            "file:///C:/Source/Z%C3%BCrich%20or%20Zurich%20(%CB%88zj%CA%8A%C9%99r%C9%AAk,/Code/resources/app/plugins/c%23/plugin.json",
            r"C:\Source\Zürich or Zurich (ˈzjʊərɪk,\Code\resources\app\plugins\c#\plugin.json",
        ),
        (
            "file:///C:/Users/Abd-al-Haseeb%27s_Dell/Studio/w3mage/wp-content/database.ht.sqlite",
            r"C:\Users\Abd-al-Haseeb's_Dell\Studio\w3mage\wp-content\database.ht.sqlite",
        ),
        ("file:///C:/project/%25A0.txt", r"C:\project\%A0.txt"),
        ("file:///C:/project/%252e.txt", r"C:\project\%2e.txt"),
    ] {
        let path = PathUri::parse(uri).expect("valid file URI");
        assert_eq!(
            NativePathString::from_path_uri(&path, PathConvention::Windows)
                .map(NativePathString::into_string),
            Ok(expected.to_string()),
            "rendering {uri}"
        );
    }
}

#[test]
fn renders_windows_unc_paths_on_every_host() {
    for (uri, expected) in [
        (
            "file://server/share/src/main.rs",
            r"\\server\share\src\main.rs",
        ),
        ("file://server/share/", "\\\\server\\share\\"),
        ("file://shares/files/c%23/p.cs", r"\\shares\files\c#\p.cs"),
        (
            "file://monacotools1/certificates/SSL/",
            "\\\\monacotools1\\certificates\\SSL\\",
        ),
    ] {
        let path = PathUri::parse(uri).expect("valid file URI");
        assert_eq!(
            NativePathString::from_path_uri(&path, PathConvention::Windows)
                .map(NativePathString::into_string),
            Ok(expected.to_string()),
            "rendering {uri}"
        );
    }
}

#[test]
fn rejects_paths_incompatible_with_the_convention() {
    for (uri, convention) in [
        ("file://server/share/file.rs", PathConvention::Posix),
        ("file:///home/alice/file.rs", PathConvention::Windows),
        ("file://server/", PathConvention::Windows),
        ("file:///_:/path", PathConvention::Windows),
    ] {
        let path = PathUri::parse(uri).expect("valid file URI");
        assert!(matches!(
            NativePathString::from_path_uri(&path, convention),
            Err(NativePathStringError::IncompatibleConvention { .. })
        ));
    }
}

#[test]
fn rejects_opaque_fallback_paths() {
    let path = PathUri::parse("file:///%00/bad/path/YQ").expect("canonical opaque fallback URI");

    for convention in [PathConvention::Posix, PathConvention::Windows] {
        assert_eq!(
            NativePathString::from_path_uri(&path, convention),
            Err(NativePathStringError::OpaqueFallback {
                path: path.to_string(),
            })
        );
    }
}

#[test]
fn rejects_non_utf8_paths() {
    for uri in ["file:///tmp/non-utf8-%FF", "file:///tmp/non-utf8-%A0"] {
        let path = PathUri::parse(uri).expect("valid file URI");

        assert!(matches!(
            NativePathString::from_path_uri(&path, PathConvention::Posix),
            Err(NativePathStringError::NonUtf8 { .. })
        ));
    }
}

#[test]
fn rejects_encoded_separators() {
    for (uri, convention) in [
        ("file:///tmp/a%2Fb", PathConvention::Posix),
        ("file:///C:/a%2Fb", PathConvention::Windows),
        ("file:///C:/a%5Cb", PathConvention::Windows),
    ] {
        let path = PathUri::parse(uri).expect("valid file URI");
        assert!(matches!(
            NativePathString::from_path_uri(&path, convention),
            Err(NativePathStringError::EncodedSeparator { .. })
        ));
    }
}

#[test]
fn rejects_invalid_windows_components() {
    for uri in [
        "file:///C:/a%3Fb",
        "file:///C:/a%2Ab",
        "file:///C:/trailing.",
        "file:///C:/trailing%20",
        "file:///C:/control-%01",
        "file://server/sh%3Fare/file.rs",
    ] {
        let path = PathUri::parse(uri).expect("valid file URI");
        assert!(matches!(
            NativePathString::from_path_uri(&path, PathConvention::Windows),
            Err(NativePathStringError::InvalidWindowsComponent { .. })
        ));
    }
}

#[test]
fn serializes_as_a_string() {
    let path = PathUri::parse("file:///workspace/src/lib.rs").expect("valid file URI");
    let rendered = NativePathString::from_path_uri(&path, PathConvention::Posix)
        .expect("POSIX URI should render");

    assert_eq!(
        serde_json::to_string(&rendered).expect("rendered path should serialize"),
        r#""/workspace/src/lib.rs""#
    );
}

#[test]
fn raw_construction_and_deserialization_preserve_foreign_spelling() {
    let expected = NativePathString::new(r"C:\workspace\project");
    let deserialized: NativePathString =
        serde_json::from_str(r#""C:\\workspace\\project""#).expect("native path string");

    assert_eq!(deserialized, expected);
    assert_eq!(deserialized.as_str(), r"C:\workspace\project");
}

#[test]
fn posix_native_paths_round_trip_through_path_uri() {
    for value in [
        "/",
        "/home/alice/a file.rs",
        "/tmp/",
        "/tmp/%A0.txt",
        "/tmp/☃",
        "/tmp/a\\b",
    ] {
        let native = NativePathString::new(value);
        let path = native
            .to_path_uri(PathConvention::Posix)
            .expect("absolute POSIX path should parse");

        assert_eq!(
            NativePathString::from_path_uri(&path, PathConvention::Posix),
            Ok(native),
            "round-tripping {value}"
        );
    }
}

#[test]
fn windows_native_paths_round_trip_through_path_uri() {
    for value in [
        r"C:\",
        r"C:\Users\Alice Smith\src\main.rs",
        r"d:\snowman\☃",
        r"C:\test with %25\c#code",
        r"\\server\share\src\main.rs",
        "\\\\server\\share\\",
    ] {
        let native = NativePathString::new(value);
        let path = native
            .to_path_uri(PathConvention::Windows)
            .expect("absolute Windows path should parse");

        assert_eq!(
            NativePathString::from_path_uri(&path, PathConvention::Windows),
            Ok(native),
            "round-tripping {value}"
        );
    }
}

#[test]
fn native_path_strings_normalize_navigation_components() {
    for (value, convention, expected_uri, expected_native) in [
        (
            "/workspace/src/../README.md",
            PathConvention::Posix,
            "file:///workspace/README.md",
            "/workspace/README.md",
        ),
        (
            "/../../workspace/./README.md",
            PathConvention::Posix,
            "file:///workspace/README.md",
            "/workspace/README.md",
        ),
        (
            r"C:\workspace\src\..\README.md",
            PathConvention::Windows,
            "file:///C:/workspace/README.md",
            r"C:\workspace\README.md",
        ),
        (
            r"\\server\share\src\..\README.md",
            PathConvention::Windows,
            "file://server/share/README.md",
            r"\\server\share\README.md",
        ),
    ] {
        let path = NativePathString::new(value)
            .to_path_uri(convention)
            .expect("absolute native path should parse");

        assert_eq!(path.to_string(), expected_uri, "parsing {value}");
        assert_eq!(
            NativePathString::from_path_uri(&path, convention).map(NativePathString::into_string),
            Ok(expected_native.to_string()),
            "rendering normalized {value}"
        );
    }
}

#[test]
fn native_path_string_rejects_invalid_absolute_paths() {
    for (value, convention) in [
        ("relative/path", PathConvention::Posix),
        ("relative\\path", PathConvention::Windows),
        (r"C:relative", PathConvention::Windows),
        (r"\\server", PathConvention::Windows),
        (r"C:\invalid?name", PathConvention::Windows),
        (r"C:\trailing.", PathConvention::Windows),
        (r"C:\workspace\D:\file.rs", PathConvention::Windows),
        ("/tmp/null\0byte", PathConvention::Posix),
        ("C:\\null\0byte", PathConvention::Windows),
    ] {
        assert!(matches!(
            NativePathString::new(value).to_path_uri(convention),
            Err(NativePathStringError::InvalidNativePath { .. })
        ));
    }
}

#[test]
fn resolves_posix_paths_without_host_path_rules() {
    let base = PathUri::parse("file:///workspace/src").expect("base URI");

    for (path, expected) in [
        ("", "file:///workspace/src"),
        ("../README.md", "file:///workspace/README.md"),
        ("../../../README.md", "file:///README.md"),
        ("/tmp/output", "file:///tmp/output"),
        (
            r"generated\output",
            "file:///workspace/src/generated%5Coutput",
        ),
    ] {
        assert_eq!(
            base.resolve_native(path, PathConvention::Posix),
            Ok(PathUri::parse(expected).expect("resolved URI")),
            "resolving {path}"
        );
    }
}

#[test]
fn resolves_windows_drive_and_unc_paths_without_host_path_rules() {
    let drive = PathUri::parse("file:///C:/workspace/src").expect("drive URI");
    let share = PathUri::parse("file://server/share/workspace/src").expect("share URI");

    for (base, path, expected) in [
        (&drive, "", "file:///C:/workspace/src"),
        (&drive, r"..\README.md", "file:///C:/workspace/README.md"),
        (&drive, r"\logs\output.txt", "file:///C:/logs/output.txt"),
        (&drive, r"D:\other\file.txt", "file:///D:/other/file.txt"),
        (
            &drive,
            r"\\other\share\file.txt",
            "file://other/share/file.txt",
        ),
        (
            &share,
            r"..\..\..\README.md",
            "file://server/share/README.md",
        ),
        (
            &share,
            r"\logs\output.txt",
            "file://server/share/logs/output.txt",
        ),
    ] {
        assert_eq!(
            base.resolve_native(path, PathConvention::Windows),
            Ok(PathUri::parse(expected).expect("resolved URI")),
            "resolving {path}"
        );
    }
}

#[test]
fn relative_resolution_rejects_incompatible_and_opaque_bases() {
    let posix = PathUri::parse("file:///workspace").expect("POSIX URI");
    let opaque = PathUri::parse("file:///%00/bad/path/YQ").expect("opaque fallback URI");

    assert!(matches!(
        posix.resolve_native("child", PathConvention::Windows),
        Err(NativePathStringError::IncompatibleConvention { .. })
    ));
    assert!(matches!(
        opaque.resolve_native("child", PathConvention::Posix),
        Err(NativePathStringError::OpaqueFallback { .. })
    ));
    assert_eq!(
        opaque.resolve_native("/tmp", PathConvention::Posix),
        Ok(PathUri::parse("file:///tmp").expect("absolute URI"))
    );
}
