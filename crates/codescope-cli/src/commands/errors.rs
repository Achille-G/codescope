use std::fmt;

#[derive(Debug)]
pub(crate) struct NoResultsError;

impl fmt::Display for NoResultsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "No results")
    }
}

impl std::error::Error for NoResultsError {}
