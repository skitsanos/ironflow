use reqwest::dns::{Name, Resolve, Resolving};

use crate::util::outbound::Guard;

/// The resolver of the shared guarded client. That client outlives a request,
/// so the policy and the operator allowlist are read for each lookup; the
/// guard then validates and pins the answer.
#[derive(Clone, Debug, Default)]
pub(super) struct GuardedDnsResolver;

impl Resolve for GuardedDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        match Guard::resolve(true) {
            Ok(Some(guard)) => guard.dns_resolver().resolve(name),
            Ok(None) => Box::pin(async { Err("the destination guard is not available".into()) }),
            Err(error) => Box::pin(async move { Err(error.into()) }),
        }
    }
}
