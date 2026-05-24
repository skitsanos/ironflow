mod email;
mod slack;

pub use email::SendEmailNode;
pub use slack::SlackNotificationNode;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(SendEmailNode));
    registry.register(Arc::new(SlackNotificationNode));
}
