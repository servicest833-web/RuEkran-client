use hbb_common::{bail, config::Config, ResultType};

fn valid_numeric_id(value: &str) -> bool {
    (6..=16).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_digit())
        && !value.starts_with('0')
}

fn authorized_id(signed_text: &str, current: &str, fingerprint: &str) -> ResultType<String> {
    use hbb_common::sodiumoxide::crypto::sign;
    use std::time::{SystemTime, UNIX_EPOCH};
    if signed_text.len() > 4096 {
        bail!("Смена ID разрешена только из панели администратора RuЭкран.");
    }
    let signed = crate::decode64(signed_text)?;
    let key = sign::PublicKey(*include_bytes!("../../../res/admin-public-key.bin"));
    let raw = sign::verify(&signed, &key).map_err(|_| hbb_common::anyhow::anyhow!("Недействительное разрешение администратора"))?;
    let ticket: serde_json::Value = serde_json::from_slice(&raw)?;
    let old = ticket["old"].as_str().unwrap_or_default();
    let new = ticket["new"].as_str().unwrap_or_default();
    let expiry = ticket["expires"].as_u64().unwrap_or_default();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if ticket["purpose"] != "ruekran-id-change-v1" || !valid_numeric_id(old)
        || !valid_numeric_id(new) || expiry < now || expiry > now + 600 {
        bail!("Разрешение недействительно или устарело. Запросите новое в панели.");
    }
    if current != old || ticket["fingerprint"].as_str().unwrap_or_default() != fingerprint
        || fingerprint.is_empty() {
        bail!("Прежний ID или ключ устройства не совпадает. Изменение отменено.");
    }
    Ok(new.to_owned())
}

pub fn apply_service_ticket(ticket: &str) -> ResultType<()> {
    if crate::get_app_name() != "RuEkran" || Config::is_disable_change_id() {
        bail!("Смена ID запрещена политикой клиента.");
    }
    let fingerprint = crate::common::pk_to_fingerprint(Config::get_key_pair().1);
    let new = authorized_id(ticket, &Config::get_id(), &fingerprint)?;
    Config::set_key_confirmed(false);
    Config::set_id(&new);
    Ok(())
}

fn change(args: &[String]) -> ResultType<()> {
    if args.len() != 1 {
        bail!("Смена ID разрешена только из панели администратора RuЭкран.");
    }
    if crate::get_app_name() != "RuEkran" || !super::is_cur_exe_the_installed() {
        bail!("Для смены ID сначала установите новую версию RuЭкран на этом компьютере.");
    }
    if !super::is_elevated(None)? || Config::is_disable_change_id() {
        bail!("Смена ID требует прав администратора и разрешения политики клиента.");
    }
    let current = crate::ipc::get_config("id")?.unwrap_or_default();
    authorized_id(&args[0], &current, &crate::ipc::get_fingerprint())?;
    crate::ipc::set_config("admin-id-ticket", args[0].clone())?;
    Ok(())
}

pub fn run(args: &[String]) {
    let message = match change(args) {
        Ok(()) => "Новый ID передан службе RuЭкран. Дождитесь подтверждения регистрации в панели управления.".to_owned(),
        Err(error) => format!("Смена ID не выполнена: {}", error),
    };
    super::message_box(&message);
}

pub fn start_remote_delivery() {
    use hbb_common::log;
    use sha2::{Digest, Sha256};
    use std::{io::Read, time::Duration};
    if crate::get_app_name() != "RuEkran" || !super::is_cur_exe_the_installed() {
        return;
    }
    std::thread::spawn(|| loop {
        let result = (|| -> ResultType<()> {
            if Config::is_disable_change_id() { return Ok(()); }
            let fingerprint = crate::common::pk_to_fingerprint(Config::get_key_pair().1);
            if fingerprint.is_empty() { return Ok(()); }
            let address = format!("https://ruekran.ru/id-commands/{:x}.json", Sha256::digest(fingerprint.as_bytes()));
            let mut builder = reqwest::blocking::Client::builder().use_rustls_tls().https_only(true)
                .no_proxy().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none());
            if let Some(config) = Config::get_socks() {
                let proxy = if config.proxy.contains("://") { config.proxy } else { format!("socks5://{}", config.proxy) };
                builder = builder.proxy(reqwest::Proxy::all(proxy)?.basic_auth(&config.username, &config.password));
            }
            let response = builder.build()?.get(address).send()?;
            if response.status() == reqwest::StatusCode::NOT_FOUND { return Ok(()); }
            let mut bytes = Vec::new();
            response.error_for_status()?.take(8193).read_to_end(&mut bytes)?;
            if bytes.len() > 8192 { bail!("Oversized administrator command"); }
            let envelope: serde_json::Value = serde_json::from_slice(&bytes)?;
            let ticket = envelope["ticket"].as_str().unwrap_or_default();
            let current = Config::get_id();
            let next = authorized_id(ticket, &current, &fingerprint)?;
            apply_service_ticket(ticket)?;
            log::info!("Remote administrator-authorized ID change applied: {} -> {}", current, next);
            Ok(())
        })();
        if let Err(error) = result {
            log::trace!("Administrator command delivery: {}", error);
        }
        std::thread::sleep(Duration::from_secs(30));
    });
}

#[cfg(test)]
mod tests {
    use super::valid_numeric_id;

    #[test]
    fn numeric_id_accepts_ten_digits_without_u32_truncation() {
        assert!(valid_numeric_id("9234160955"));
        for invalid in ["12345", "0123456", "C777777", "12345678901234567", "１２３４５６"] {
            assert!(!valid_numeric_id(invalid));
        }
    }
}
