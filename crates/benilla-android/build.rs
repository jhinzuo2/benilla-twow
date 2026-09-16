//! Same job as `crates/benilla/build.rs`: stamp the commit this .so was built from. See
//! `benilla-buildstamp` for the rule and decision 0993 for why the stamp lives in a thin shim
//! rather than in `benilla-app` directly — that reasoning applies identically to this entry
//! point, so it is duplicated here rather than shared, same as the desktop shim.

fn main() {
    benilla_buildstamp::emit();
}
