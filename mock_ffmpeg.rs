fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    // If image2pipe is used, stream some gray frames slowly
    if args.windows(2).any(|w| w[0] == "-f" && w[1] == "image2pipe") {
        let frame = vec![128u8; 640 * 360 * 4];
        let mut out = std::io::stdout();
        use std::io::Write;
        for _ in 0..60 { // Send frames for a few seconds
            if out.write_all(&frame).is_err() { break; }
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
    } else {
        // Mock success for convert_to_wav or export
        if let Some(out_pos) = args.iter().position(|a| a == "mock_output.mp4" || a.ends_with(".wav") || a.ends_with(".mp4") || a.ends_with(".gif")) {
            if out_pos < args.len() {
                let _ = std::fs::write(&args[out_pos], "mock content");
            }
        }
    }
}
