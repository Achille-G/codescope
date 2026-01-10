/**
 * User management service for CRUD operations.
 *
 * Handles user creation, updates, deletion, and profile management.
 */

import { User, UserProfile, UserRole } from '../models/user';
import { DatabaseConnection } from '../utils/database';
import { EmailService } from './email_service';
import { ValidationError } from '../utils/errors';

export interface CreateUserInput {
    username: string;
    email: string;
    password: string;
    displayName?: string;
}

export interface UpdateUserInput {
    displayName?: string;
    email?: string;
    avatar?: string;
}

/**
 * Service for managing user accounts and profiles.
 */
export class UserService {
    private db: DatabaseConnection;
    private emailService: EmailService;

    constructor(db: DatabaseConnection, emailService: EmailService) {
        this.db = db;
        this.emailService = emailService;
    }

    /**
     * Create a new user account.
     *
     * @param input - User creation data
     * @returns The created user
     * @throws ValidationError if username or email already exists
     */
    async createUser(input: CreateUserInput): Promise<User> {
        // Validate uniqueness
        const existingUsername = await this.db.findUserByUsername(input.username);
        if (existingUsername) {
            throw new ValidationError('Username already taken');
        }

        const existingEmail = await this.db.findUserByEmail(input.email);
        if (existingEmail) {
            throw new ValidationError('Email already registered');
        }

        // Create user
        const user = await this.db.createUser({
            username: input.username,
            email: input.email,
            passwordHash: await this.hashPassword(input.password),
            displayName: input.displayName || input.username,
            role: UserRole.User,
            createdAt: new Date(),
        });

        // Send welcome email
        await this.emailService.sendWelcomeEmail(user);

        return user;
    }

    /**
     * Get a user by their ID.
     */
    async getUserById(userId: string): Promise<User | null> {
        return this.db.findUserById(userId);
    }

    /**
     * Get a user's public profile.
     */
    async getUserProfile(userId: string): Promise<UserProfile | null> {
        const user = await this.db.findUserById(userId);
        if (!user) {
            return null;
        }

        return {
            id: user.id,
            username: user.username,
            displayName: user.displayName,
            avatar: user.avatar,
            bio: user.bio,
            joinedAt: user.createdAt,
        };
    }

    /**
     * Update a user's profile.
     */
    async updateUser(userId: string, input: UpdateUserInput): Promise<User> {
        const user = await this.db.findUserById(userId);
        if (!user) {
            throw new ValidationError('User not found');
        }

        // If email is changing, verify uniqueness
        if (input.email && input.email !== user.email) {
            const existingEmail = await this.db.findUserByEmail(input.email);
            if (existingEmail) {
                throw new ValidationError('Email already in use');
            }
        }

        return this.db.updateUser(userId, input);
    }

    /**
     * Delete a user account.
     */
    async deleteUser(userId: string): Promise<void> {
        await this.db.deleteUser(userId);
    }

    /**
     * Search for users by username or display name.
     */
    async searchUsers(query: string, limit = 10): Promise<User[]> {
        return this.db.searchUsers(query, limit);
    }

    private async hashPassword(password: string): Promise<string> {
        // In production, use bcrypt or argon2
        return password; // Simplified for demo
    }
}
