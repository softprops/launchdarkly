extern crate launchdarkly;
extern crate tokio_core;

use tokio_core::reactor::Core;
use launchdarkly::Client;

fn main() {
  let mut core = Core::new().expect("reactor fail");
  let ld = Client::new(env!("LD_KEY"), &core.handle());
  for flag in core
    .run(ld.flags("default", &Default::default()))
    .expect("invalid request")
    .items
  {
    println!(
      "{:50}\t({})\t{}",
      flag.name,
      flag.kind,
      flag
        .variations
        .iter()
        .map(|v| v.value.to_string())
        .collect::<Vec<_>>()
        .join(", ")
    )
  }
}
