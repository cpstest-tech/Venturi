use super::check_imports;

#[test]
fn react_and_remotion_imports_are_allowed() {
    let source = "import React, {useState} from 'react';\n\
        import {AbsoluteFill, interpolate} from \"remotion\";\n\
        import {Circle} from '@remotion/shapes';\n\
        const from = 3; export default () => null;";
    assert_eq!(check_imports(source), Ok(()));
}

#[test]
fn other_imports_are_rejected() {
    for source in [
        "import key from 'C:/Users/me/.config/gcloud/credentials.json';",
        "import fs from 'fs';",
        "import './Other';",
        "export {x} from '../secret';",
        "const fs = require('fs');",
        "const m = await import('remotion');",
    ] {
        let error = check_imports(source).unwrap_err();
        assert!(
            error.starts_with("TANDEM_COMPOSITION_ERROR:"),
            "{source}: {error}"
        );
    }
}
