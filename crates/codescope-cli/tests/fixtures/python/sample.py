"""
Sample Python module for testing codescope chunking.
"""

from dataclasses import dataclass
from typing import List, Optional


@dataclass
class User:
    """Represents a user in the system."""
    id: str
    username: str
    email: str


class UserRepository:
    """Repository for managing users."""

    def __init__(self):
        """Initialize the repository with an empty user list."""
        self._users: List[User] = []

    def add(self, user: User) -> None:
        """
        Add a user to the repository.

        Args:
            user: The user to add
        """
        self._users.append(user)

    def find_by_id(self, user_id: str) -> Optional[User]:
        """
        Find a user by their ID.

        Args:
            user_id: The ID to search for

        Returns:
            The user if found, None otherwise
        """
        for user in self._users:
            if user.id == user_id:
                return user
        return None

    def find_by_username(self, username: str) -> Optional[User]:
        """Find a user by username."""
        for user in self._users:
            if user.username == username:
                return user
        return None


def calculate_sum(numbers: List[int]) -> int:
    """Calculate the sum of a list of numbers."""
    return sum(numbers)


def calculate_average(numbers: List[float]) -> float:
    """Calculate the average of a list of numbers."""
    if not numbers:
        return 0.0
    return sum(numbers) / len(numbers)


def greet(name: str) -> str:
    """Return a greeting message."""
    return f"Hello, {name}!"
