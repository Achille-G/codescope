//! Sample Rust module for testing codescope chunking.

use std::collections::HashMap;

/// Represents a user in the system.
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
}

impl User {
    /// Create a new user with the given details.
    pub fn new(id: String, username: String, email: String) -> Self {
        Self { id, username, email }
    }

    /// Get a display name for the user.
    pub fn display_name(&self) -> &str {
        &self.username
    }
}

/// Repository for managing users.
pub struct UserRepository {
    users: HashMap<String, User>,
}

impl UserRepository {
    /// Create a new empty repository.
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
        }
    }

    /// Add a user to the repository.
    pub fn add(&mut self, user: User) {
        self.users.insert(user.id.clone(), user);
    }

    /// Find a user by their ID.
    pub fn find_by_id(&self, id: &str) -> Option<&User> {
        self.users.get(id)
    }

    /// Find a user by their username.
    pub fn find_by_username(&self, username: &str) -> Option<&User> {
        self.users.values().find(|u| u.username == username)
    }

    /// Get all users in the repository.
    pub fn all(&self) -> Vec<&User> {
        self.users.values().collect()
    }
}

impl Default for UserRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate the sum of a slice of numbers.
pub fn calculate_sum(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}

/// Calculate the average of a slice of numbers.
pub fn calculate_average(numbers: &[f64]) -> f64 {
    if numbers.is_empty() {
        return 0.0;
    }
    numbers.iter().sum::<f64>() / numbers.len() as f64
}

/// Return a greeting message.
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_sum() {
        assert_eq!(calculate_sum(&[1, 2, 3, 4, 5]), 15);
    }

    #[test]
    fn test_calculate_average() {
        assert_eq!(calculate_average(&[1.0, 2.0, 3.0]), 2.0);
    }

    #[test]
    fn test_greet() {
        assert_eq!(greet("World"), "Hello, World!");
    }
}
