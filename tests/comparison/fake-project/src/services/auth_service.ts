/**
 * Authentication service handling user login, logout, and session management.
 *
 * This service integrates with the JWT token system and manages user sessions
 * across the application. It supports OAuth2 providers and local authentication.
 */

import { User } from '../models/user';
import { TokenService } from './token_service';
import { DatabaseConnection } from '../utils/database';

export interface AuthCredentials {
    username: string;
    password: string;
    rememberMe?: boolean;
}

export interface AuthResult {
    success: boolean;
    user?: User;
    token?: string;
    error?: string;
}

/**
 * Main authentication service class.
 * Handles all authentication-related operations.
 */
export class AuthService {
    private tokenService: TokenService;
    private db: DatabaseConnection;

    constructor(tokenService: TokenService, db: DatabaseConnection) {
        this.tokenService = tokenService;
        this.db = db;
    }

    /**
     * Authenticate a user with username and password.
     *
     * @param credentials - The login credentials
     * @returns Promise resolving to authentication result
     */
    async login(credentials: AuthCredentials): Promise<AuthResult> {
        const { username, password, rememberMe } = credentials;

        // Validate input
        if (!username || !password) {
            return { success: false, error: 'Missing credentials' };
        }

        // Find user in database
        const user = await this.db.findUserByUsername(username);
        if (!user) {
            return { success: false, error: 'User not found' };
        }

        // Verify password
        const passwordValid = await this.verifyPassword(password, user.passwordHash);
        if (!passwordValid) {
            return { success: false, error: 'Invalid password' };
        }

        // Generate token
        const expiresIn = rememberMe ? '30d' : '24h';
        const token = this.tokenService.generateToken(user, expiresIn);

        return { success: true, user, token };
    }

    /**
     * Log out a user and invalidate their session.
     *
     * @param userId - The ID of the user to log out
     * @param token - The current authentication token
     */
    async logout(userId: string, token: string): Promise<void> {
        await this.tokenService.invalidateToken(token);
        await this.db.clearUserSession(userId);
    }

    /**
     * Refresh an authentication token.
     *
     * @param refreshToken - The refresh token
     * @returns Promise resolving to new token or null if invalid
     */
    async refreshToken(refreshToken: string): Promise<string | null> {
        const payload = this.tokenService.verifyRefreshToken(refreshToken);
        if (!payload) {
            return null;
        }

        const user = await this.db.findUserById(payload.userId);
        if (!user) {
            return null;
        }

        return this.tokenService.generateToken(user, '24h');
    }

    /**
     * Verify a password against a hash.
     */
    private async verifyPassword(password: string, hash: string): Promise<boolean> {
        // In production, use bcrypt or argon2
        return password === hash; // Simplified for demo
    }
}
