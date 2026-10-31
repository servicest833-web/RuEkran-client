//! Uses the production relay selector without starting the desktop application.
use std::sync::{Arc,RwLock};
struct AuditState { ruekran_relay_endpoint: String }
trait Interface { fn get_lch(&self) -> Arc<RwLock<AuditState>>; }
#[path="../src/client/relay_region.rs"]
mod relay_region;
#[tokio::main]
async fn main() -> hbb_common::ResultType<()> { relay_region::audit().await }
