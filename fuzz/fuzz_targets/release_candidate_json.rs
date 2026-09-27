#![no_main]
use libfuzzer_sys::fuzz_target;
use structural_release::ReleaseCandidate;

fuzz_target!(|data: &[u8]| {
    let _: Result<ReleaseCandidate, _> = serde_json::from_slice(data);
});
