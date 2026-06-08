//! 跨平台子行程建構。
//!
//! Windows 上：GUI 程式（AMAGI）叫起 console 程式（git / node 等）時，
//! 系統會自動配一個可見的 console 視窗，造成黑窗一閃。加上
//! `CREATE_NO_WINDOW` 旗標即可隱藏該視窗，stdout/stderr 仍照常透過 pipe 取得。
//! 其他平台無此概念，旗標不套用。

use std::process::Command;

/// 建立指定程式的 Command；Windows 上隱藏 console 視窗。
pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
