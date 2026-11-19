use super::npm_shim_script;

#[test]
fn npm_shim_script_is_found() {
    let shim = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\n\
        SETLOCAL\r\nCALL :find_dp0\r\n\r\n\
        IF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n)\r\n\r\n\
        endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \
        \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n";
    assert_eq!(
        npm_shim_script(shim),
        Some("node_modules\\@openai\\codex\\bin\\codex.js")
    );
}

#[test]
fn a_plain_batch_file_has_no_script() {
    assert_eq!(npm_shim_script("@echo off\r\necho hi\r\n"), None);
}
