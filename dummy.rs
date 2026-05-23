#![windows_subsystem = "windows"]
use std::process::Command;

fn main() {
    let script = r#"
Add-Type -AssemblyName PresentationFramework
[System.Windows.MessageBox]::Show('Welcome to Phantom Desktop!

The full native executable is currently being compiled on the backend server, which takes several minutes. 

You are seeing this stub because the build has not finished syncing to the downloads page yet. Please try downloading again in a few minutes!', 'Phantom', 'OK', 'Information')
    "#;
    
    Command::new("powershell")
        .args(&["-NoProfile", "-Command", script])
        .output()
        .unwrap();
}
