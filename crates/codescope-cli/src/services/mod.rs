//! Background services for codescope CLI

pub mod index_service;
pub mod scheduler;

pub use index_service::{IndexProgress, IndexService};
pub use scheduler::{EventHandle, SchedulerConfig, WorkItem, WorkScheduler, WorkType};
