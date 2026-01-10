/**
 * A service for handling user authentication
 */
export class AuthService {
    private users: Map<string, User> = new Map();

    /**
     * Login a user with username and password
     * @param username - The username
     * @param password - The password
     * @returns The authenticated user or null
     */
    public async login(username: string, password: string): Promise<User | null> {
        const user = this.users.get(username);
        if (user && user.password === password) {
            return user;
        }
        return null;
    }

    /**
     * Register a new user
     * @param username - The username
     * @param password - The password
     * @returns The created user
     */
    public register(username: string, password: string): User {
        const user: User = {
            id: Date.now().toString(),
            username,
            password,
            createdAt: new Date()
        };
        this.users.set(username, user);
        return user;
    }
}

interface User {
    id: string;
    username: string;
    password: string;
    createdAt: Date;
}

/**
 * Calculate the sum of numbers
 */
export function sum(numbers: number[]): number {
    return numbers.reduce((a, b) => a + b, 0);
}

/**
 * Calculate the average of numbers
 */
export function average(numbers: number[]): number {
    if (numbers.length === 0) return 0;
    return sum(numbers) / numbers.length;
}

const greet = (name: string) => {
    console.log(`Hello, ${name}!`);
};
