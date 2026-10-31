#[path = "../src/platform/windows_text_focus.rs"]
mod text_focus;
use librustdesk::platform;

#[cfg(test)]
#[path = "../src/server"]
mod video_budget_tests {
    use base::message_proto::ImageQuality;
    use hbb_common::{config::Config, log, protobuf::Enum};
    use std::collections::HashMap;
    mod video_qos;
}

fn main() {
    if std::env::args().any(|a| a == "--host" || a == "--cm" || a == "--check-hwcodec-config") {
        isolated_host();
        return;
    }
    let mut reporter = text_focus::TextFocusReporter::default();
    reporter.configure(2);
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 90 {
        if let Some(active) = reporter.sample(true) {
            println!("{} text_input_active={active}", start.elapsed().as_millis());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn isolated_host() {
    use hbb_common::config::{Config, APP_NAME};
    *APP_NAME.write().unwrap() = "RuEkranVerify21".to_owned();
    if std::env::args().any(|a| a == "--host") {
        use std::io::BufRead;
        let mut password = String::new();
        std::io::stdin().lock().read_line(&mut password).unwrap();
        let password = password.trim();
        assert!(password.len() >= 20);
        let id = std::env::var("RUEKRAN_VERIFY_ID").unwrap();
        assert!(id.starts_with("88") && id.len() == 10 && id.bytes().all(|b| b.is_ascii_digit()));
        Config::set_id(&id);
        Config::set_option("custom-rendezvous-server".into(), "adm.stilservice70.ru".into());
        Config::set_option("key".into(), "LJ9XMVMCFcS1HPwuV1LZDccKUK4akNJokLeMA7fiMbY=".into());
        Config::set_option("verification-method".into(), "use-permanent-password".into());
        Config::set_option("approve-mode".into(), "password".into());
        assert!(Config::set_permanent_password(password));
        assert!(Config::file().to_string_lossy().contains("RuEkranVerify21"));
        println!("VERIFY_HOST_ID={id}");
        librustdesk::start_server(true, false);
    } else if let Some(args) = librustdesk::core_main::core_main().as_mut() {
        librustdesk::ui::start(args);
    }
}
