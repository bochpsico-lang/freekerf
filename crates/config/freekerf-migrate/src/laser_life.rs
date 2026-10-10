//! `LaserLifeCounter.bin`: LaserGRBL's laser module counters, a `ListLLC` (a
//! `List<LaserLifeCounter>`) saved by `Tools.Serializer.ObjToFile`: an ASCII tag
//! header, then the `BinaryFormatter` stream.

use freekerf_protocol::LaserLifeCounter;

use crate::MigrateError;
use crate::nrbf::{self, Value};

/// Where `ObjToFile`'s header ends.
const DATA_START: &[u8] = b"<!-- OBJECT DATA BEGIN HERE-->\r\n";

/// The `BinaryFormatter` data of a file written by `Tools.Serializer.ObjToFile`.
///
/// # Errors
/// No header, or a compressed or encrypted file (LaserGRBL writes neither for its
/// counters).
pub fn object_data(bytes: &[u8]) -> Result<&[u8], MigrateError> {
    let end = bytes
        .windows(DATA_START.len())
        .take(1024)
        .position(|w| w == DATA_START)
        .ok_or_else(|| MigrateError::Content("no serializer header".into()))?;
    let header = String::from_utf8_lossy(&bytes[..end]);
    if header.contains("Encription: Enabled") || header.contains("Compression: Enabled") {
        return Err(MigrateError::Content("compressed or encrypted file".into()));
    }
    Ok(&bytes[end + DATA_START.len()..])
}

fn date(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::DateTime { ticks, .. }) => Some(nrbf::date(*ticks)),
        _ => None,
    }
}

#[allow(clippy::cast_sign_loss)]
fn ms(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::TimeSpan(t)) if *t > 0 => (*t / 10_000) as u64,
        _ => 0,
    }
}

fn text(v: Option<&Value>, default: &str) -> String {
    v.and_then(Value::as_str).unwrap_or(default).to_owned()
}

/// Reads `LaserLifeCounter.bin` into FreeKerf's counters (same fields; times in
/// milliseconds, dates as `YYYY-MM-DD`).
///
/// # Errors
/// Not a counter list.
pub fn read(bytes: &[u8]) -> Result<Vec<LaserLifeCounter>, MigrateError> {
    let root = nrbf::read(object_data(bytes)?)?;
    if !root.class().is_some_and(|c| c.contains("ListLLC")) {
        return Err(MigrateError::Content(format!(
            "not a laser counter list: {:?}",
            root.class()
        )));
    }
    let size = root.member("_size").and_then(Value::as_i64).unwrap_or(0);
    let items = root
        .member("_items")
        .and_then(Value::as_array)
        .unwrap_or_default();
    let count = usize::try_from(size).unwrap_or(0).min(items.len());
    let mut out = Vec::with_capacity(count);
    for c in &items[..count] {
        let mut counter =
            LaserLifeCounter::new(text(c.member("mGuid"), ""), text(c.member("mName"), "Default"));
        counter.brand = text(c.member("mBrand"), "Unknown");
        counter.model = text(c.member("mModuleModel"), "Unknown");
        counter.optical_power_w = c.member("mOpticalPower").and_then(Value::as_f64);
        counter.purchase_date = date(c.member("mPurchaseDate"));
        counter.monitoring_date = date(c.member("mMonitoringDate"));
        counter.death_date = date(c.member("mDeathDate"));
        counter.last_usage = date(c.member("mLastUsage"));
        counter.run_ms = ms(c.member("mTimeInRun"));
        counter.normalized_power_ms = ms(c.member("mTimeUsageNormalizedPower"));
        counter.non_zero_ms = ms(c.member("mTimeUsageNonZero"));
        if let Some(classes) = c.member("mTimeClasses").and_then(Value::as_array) {
            for (dst, src) in counter.classes_ms.iter_mut().zip(classes) {
                *dst = ms(Some(src));
            }
        }
        out.push(counter);
    }
    Ok(out)
}
