/**
 * User-related type definitions and interfaces.
 */

export enum UserRole {
    Admin = 'admin',
    Moderator = 'moderator',
    User = 'user',
    Guest = 'guest',
}

export interface User {
    id: string;
    username: string;
    email: string;
    passwordHash: string;
    displayName: string;
    avatar?: string;
    bio?: string;
    role: UserRole;
    isVerified: boolean;
    createdAt: Date;
    updatedAt: Date;
    lastLoginAt?: Date;
}

export interface UserProfile {
    id: string;
    username: string;
    displayName: string;
    avatar?: string;
    bio?: string;
    joinedAt: Date;
}

export interface UserSession {
    id: string;
    userId: string;
    token: string;
    refreshToken: string;
    expiresAt: Date;
    createdAt: Date;
    ipAddress?: string;
    userAgent?: string;
}

/**
 * Create a default user object with minimal fields.
 */
export function createDefaultUser(username: string, email: string): Partial<User> {
    return {
        username,
        email,
        displayName: username,
        role: UserRole.User,
        isVerified: false,
        createdAt: new Date(),
        updatedAt: new Date(),
    };
}

/**
 * Check if a user has admin privileges.
 */
export function isAdmin(user: User): boolean {
    return user.role === UserRole.Admin;
}

/**
 * Check if a user has moderator or higher privileges.
 */
export function isModerator(user: User): boolean {
    return user.role === UserRole.Admin || user.role === UserRole.Moderator;
}
