use hbb_common::{anyhow::anyhow, bail, config::Config, log, sodiumoxide::crypto::sign, ResultType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, sync::Mutex, time::Duration};

const FEED: &str = "https://ruekran.ru/updates/windows-x86.json";
const MAX_SIZE: u64 = 128 * 1024 * 1024;

#[derive(Clone, Deserialize)]
struct Release {
    purpose: String,
    version: String,
    build: u64,
    size: u64,
    sha256: String,
    url: String,
}

#[derive(Default, Serialize)]
struct Status {
    state: String,
    version: String,
    percent: u64,
    message: String,
    #[serde(skip)]
    release: Option<Release>,
}

lazy_static::lazy_static! {
    static ref STATUS: Mutex<Status> = Mutex::new(Status::default());
}

fn current_build() -> u64 {
    parse_build(include_str!("../../../res/ruekran-version-numeric.txt")).unwrap_or(u64::MAX)
}

fn parse_build(value: &str) -> Option<u64> {
    let mut parts = value.trim().split('.');
    for _ in 0..3 { parts.next()?.parse::<u16>().ok()?; }
    let build = parts.next()?.parse::<u16>().ok()?;
    if parts.next().is_some() || build == 0 { return None; }
    Some(u64::from(build))
}

fn parse_release(bytes: &[u8]) -> ResultType<Release> {
    if bytes.len() > 16384 { bail!("Слишком большой ответ сервера обновлений"); }
    let envelope: serde_json::Value = serde_json::from_slice(bytes)?;
    let signed = crate::decode64(envelope["signed"].as_str().unwrap_or_default())?;
    let key = sign::PublicKey(*include_bytes!("../../../res/update-public-key.bin"));
    let raw = sign::verify(&signed, &key).map_err(|_| anyhow!("Подпись обновления не подтверждена"))?;
    let release: Release = serde_json::from_slice(&raw)?;
    if release.purpose != "ruekran-windows-update-v1"
        || release.version.is_empty() || release.version.len() > 48
        || !release.version.bytes().all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
        || release.build == 0 || !(1024..=MAX_SIZE).contains(&release.size)
        || release.sha256.len() != 64 || !release.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        || release.url != format!("https://ruekran.ru/releases/RuEkran-Windows-x86-x64-{}.exe", release.build) {
        bail!("Неверные сведения о выпуске RuЭкран");
    }
    Ok(release)
}

fn client() -> ResultType<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder().use_rustls_tls().https_only(true)
        .no_proxy().connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(config) = Config::get_socks() {
        let address = if config.proxy.contains("://") { config.proxy } else { format!("socks5://{}", config.proxy) };
        let proxy = reqwest::Proxy::all(address)?.basic_auth(&config.username, &config.password);
        builder = builder.proxy(proxy);
    }
    Ok(builder.build()?)
}

fn fetch() -> ResultType<Release> {
    let mut bytes = Vec::new();
    client()?.get(FEED).send()?.error_for_status()?.take(16385).read_to_end(&mut bytes)?;
    parse_release(&bytes)
}

pub fn start() {
    if crate::get_app_name() != "RuEkran" { return; }
    std::thread::spawn(|| loop {
        command("check");
        std::thread::sleep(Duration::from_secs(6 * 3600));
    });
}

pub fn command(action: &str) -> String {
    if action == "check" || action == "install" {
        let mut state = STATUS.lock().unwrap();
        if !matches!(state.state.as_str(), "checking" | "downloading" | "installing") {
            if action == "check" {
                state.state = "checking".into();
                drop(state);
                let spawn = std::thread::Builder::new().name("ruekran-update-check".into()).spawn(|| {
                    struct ResetChecking;
                    impl Drop for ResetChecking {
                        fn drop(&mut self) {
                            if let Ok(mut state) = STATUS.lock() {
                                if state.state == "checking" { state.state = "idle".into(); }
                            }
                        }
                    }
                    let _reset = ResetChecking;
                    let result = fetch();
                    let mut state = STATUS.lock().unwrap();
                    match result {
                        Ok(release) => {
                            state.version = release.version.clone();
                            state.state = if release.build > current_build() { "available" } else { "current" }.into();
                            state.release = Some(release);
                            state.message.clear();
                        }
                        Err(error) => {
                            log::debug!("RuEkran update check failed: {}", error);
                            state.state = if state.release.as_ref().map_or(false, |r| r.build > current_build()) { "available" } else { "idle" }.into();
                        }
                    }
                });
                if spawn.is_err() { STATUS.lock().unwrap().state = "idle".into(); }
            } else if state.release.as_ref().map_or(false, |r| r.build > current_build()) {
                state.state = "downloading".into();
                state.percent = 0;
                state.message.clear();
                drop(state);
                std::thread::spawn(|| {
                    if let Err(error) = download_and_install() {
                        log::error!("RuEkran update failed: {}", error);
                        let mut state = STATUS.lock().unwrap();
                        state.state = "error".into();
                        state.message = format!("Не удалось обновить: {}", error);
                    }
                });
            }
        }
    }
    serde_json::to_string(&*STATUS.lock().unwrap()).unwrap_or_else(|_| "{}".into())
}

fn verify_file(path: &Path, release: &Release) -> ResultType<()> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() != release.size { bail!("Размер файла обновления не совпадает"); }
    let mut hash = Sha256::new();
    let mut reader = std::io::BufReader::new(file);
    let mut buffer = [0u8; 65536];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 { break; }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != release.sha256.to_lowercase() {
        bail!("Контрольная сумма файла обновления не совпадает");
    }
    Ok(())
}

fn download_and_install() -> ResultType<()> {
    // Recheck the signed feed before starting a download; never use an unsigned URL from UI.
    let release = fetch()?;
    if release.build <= current_build() { bail!("Новая версия больше не предлагается сервером"); }
    let directory = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(|| anyhow!("Нет папки обновлений Windows"))?)
        .join("RuEkran").join("updates").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&directory)?;
    let path = directory.join("RuEkran-Windows-x86-x64.exe");
    let result = (|| -> ResultType<()> {
        let mut response = client()?.get(&release.url).timeout(Duration::from_secs(180)).send()?.error_for_status()?;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
        let mut buffer = [0u8; 65536];
        let mut total = 0;
        loop {
            let count = response.read(&mut buffer)?;
            if count == 0 { break; }
            total += count as u64;
            if total > release.size { bail!("Получен файл неверного размера"); }
            file.write_all(&buffer[..count])?;
            STATUS.lock().unwrap().percent = total * 100 / release.size;
        }
        file.sync_all()?;
        drop(file);
        verify_file(&path, &release)?;
        STATUS.lock().unwrap().state = "installing".into();
        install(&path, &release)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&directory);
    }
    result
}

fn install(path: &Path, release: &Release) -> ResultType<()> {
    if super::is_installed() {
        // Keep every pathname component and the verified image immutable until
        // ShellExecute has completed the elevation/launch handoff.
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use winapi::um::winbase::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
        use winapi::um::winnt::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ};
        let mut guards = Vec::new();
        let directories: Vec<_> = path.ancestors().skip(1).collect();
        for directory in directories.into_iter().rev() {
            let guard = fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).open(directory)?;
            if guard.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                bail!("Папка обновления не должна быть ссылкой");
            }
            guards.push(guard);
        }
        let image = fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(path)?;
        if image.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            bail!("Файл обновления не должен быть ссылкой");
        }
        guards.push(image);
        verify_file(path, release)?;
        let (_, _, _, installed) = super::get_install_info();
        super::validate_install_value(&installed)?;
        if !super::run_uac(&path.to_string_lossy(), &format!("--ruekran-update-installed \"{}\"", installed))? {
            bail!("Подтвердите права Windows и повторите обновление");
        }
        drop(guards);
    } else {
        let origin = PathBuf::from(std::env::var_os("RUEKRAN_PORTABLE_PATH")
            .ok_or_else(|| anyhow!("Для обновления запустите единый portable EXE RuЭкран"))?);
        if !fs::symlink_metadata(&origin)?.is_file() || origin.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("exe")) != Some(true) {
            bail!("Не найден исходный portable EXE");
        }
        let staged = origin.with_extension(format!("{}.new", uuid::Uuid::new_v4()));
        let replace = (|| -> ResultType<()> {
            fs::copy(path, &staged)?;
            verify_file(&staged, release)?;
            fs::rename(&staged, &origin)?;
            Ok(())
        })();
        if replace.is_err() { let _ = fs::remove_file(&staged); }
        replace?;
        std::process::Command::new(&origin).args(["--ruekran-wait-pid", &std::process::id().to_string()]).spawn()?;
        if let Err(error) = super::send_close("") { log::debug!("Portable background close: {}", error); }
        std::process::exit(0);
    }
    Ok(())
}

pub fn verify_cli(args: &[String]) {
    let result = (|| -> ResultType<()> {
        if args.len() != 3 { bail!("Usage: --verify-ruekran-update manifest.json release.exe"); }
        let release = parse_release(&fs::read(&args[1])?)?;
        if release.build < current_build() { bail!("Откат на предыдущую сборку запрещён"); }
        verify_file(Path::new(&args[2]), &release)?;
        println!("Verified RuEkran {} build {}", release.version, release.build);
        Ok(())
    })();
    if let Err(error) = result { eprintln!("{}", error); std::process::exit(1); }
}
