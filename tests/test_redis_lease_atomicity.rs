#![cfg(feature = "redis")]

#[path = "redis_lease_atomicity/arguments.rs"]
mod arguments;
#[path = "redis_lease_atomicity/boundaries.rs"]
mod boundaries;
#[path = "redis_lease_atomicity/faults.rs"]
mod faults;
#[path = "support/redis.rs"]
mod redis_support;
#[path = "redis_lease_atomicity/support.rs"]
mod support;
