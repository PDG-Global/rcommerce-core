// R Commerce API Client - Proxy Version
// This version works with rcommerce-demo-server which handles API authentication
// No API keys in browser - all requests go through the proxy

const API_BASE_URL = '/api';  // Relative URL - goes through proxy

// Storage keys
const AUTH_TOKEN_KEY = 'rc_token';
const REFRESH_TOKEN_KEY = 'rc_refresh_token';
const CUSTOMER_KEY = 'rc_customer';
const CART_KEY = 'rc_cart';
const ORDERS_KEY = 'rc_orders';

// API Client
const api = {
    // Authentication
    async login(email, password) {
        const response = await fetch(`${API_BASE_URL}/v1/auth/login`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ email, password })
        });
        
        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || 'Login failed');
        }
        
        const data = await response.json();
        this.setAuth(data.access_token, data.refresh_token);
        
        // Fetch customer profile
        await this.fetchCustomerProfile();
        
        return data;
    },
    
    async register(customerData) {
        const response = await fetch(`${API_BASE_URL}/v1/auth/register`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(customerData)
        });
        
        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || 'Registration failed');
        }
        
        const data = await response.json();
        this.setAuth(data.access_token, data.refresh_token);
        localStorage.setItem(CUSTOMER_KEY, JSON.stringify(data.customer));
        return data;
    },
    
    async refreshToken() {
        const refreshToken = localStorage.getItem(REFRESH_TOKEN_KEY);
        if (!refreshToken) throw new Error('No refresh token');
        
        const response = await fetch(`${API_BASE_URL}/v1/auth/refresh`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ refresh_token: refreshToken })
        });
        
        if (!response.ok) {
            this.clearAuth();
            throw new Error('Session expired');
        }
        
        const data = await response.json();
        this.setAuth(data.access_token, data.refresh_token);
        return data;
    },
    
    // Password Reset
    async requestPasswordReset(email) {
        const response = await fetch(`${API_BASE_URL}/v1/auth/password-reset`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ email })
        });
        
        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || 'Failed to request password reset');
        }
        return response.json();
    },
    
    async confirmPasswordReset(token, newPassword) {
        const response = await fetch(`${API_BASE_URL}/v1/auth/password-reset/confirm`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ token, password: newPassword })
        });
        
        if (!response.ok) {
            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || 'Failed to reset password');
        }
        return response.json();
    },
    
    // Customer
    async fetchCustomerProfile() {
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/customers`);
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.fetchCustomerProfile();
            }
            throw new Error('Failed to fetch profile');
        }
        
        const data = await response.json();
        const customer = data.customer || data.customers?.[0] || data;
        localStorage.setItem(CUSTOMER_KEY, JSON.stringify(customer));
        return customer;
    },
    
    // Products (through proxy - no API key needed in browser)
    async getProducts() {
        const response = await fetch(`${API_BASE_URL}/v1/products`);
        
        if (!response.ok) {
            throw new Error('Failed to fetch products');
        }
        
        const data = await response.json();
        return data.products || data;
    },
    
    async getProduct(id) {
        const response = await fetch(`${API_BASE_URL}/v1/products/${id}`);
        
        if (!response.ok) {
            throw new Error('Failed to fetch product');
        }
        
        const data = await response.json();
        return data.product || data;
    },
    
    // Cart
    async getCart() {
        if (!this.isAuthenticated()) {
            return JSON.parse(localStorage.getItem(CART_KEY)) || [];
        }
        
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/carts`);
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.getCart();
            }
            throw new Error('Failed to fetch cart');
        }
        
        const data = await response.json();
        return data.cart?.items || data.items || [];
    },
    
    async addToCart(productId, quantity = 1) {
        if (!this.isAuthenticated()) {
            const cart = JSON.parse(localStorage.getItem(CART_KEY)) || [];
            const existing = cart.find(item => item.product_id === productId);
            
            if (existing) {
                existing.quantity += quantity;
            } else {
                cart.push({ product_id: productId, quantity });
            }
            
            localStorage.setItem(CART_KEY, JSON.stringify(cart));
            return cart;
        }
        
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/carts/items`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ product_id: productId, quantity })
        });
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.addToCart(productId, quantity);
            }
            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || 'Failed to add to cart');
        }
        
        return response.json();
    },
    
    async updateCartItem(itemId, quantity) {
        if (!this.isAuthenticated()) {
            const cart = JSON.parse(localStorage.getItem(CART_KEY)) || [];
            const item = cart.find(i => i.product_id === itemId || i.id === itemId);
            
            if (item) {
                if (quantity <= 0) {
                    const idx = cart.indexOf(item);
                    cart.splice(idx, 1);
                } else {
                    item.quantity = quantity;
                }
            }
            
            localStorage.setItem(CART_KEY, JSON.stringify(cart));
            return cart;
        }
        
        if (quantity <= 0) {
            return this.removeCartItem(itemId);
        }
        
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/carts/items/${itemId}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ quantity })
        });
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.updateCartItem(itemId, quantity);
            }
            throw new Error('Failed to update cart');
        }
        
        return response.json();
    },
    
    async removeCartItem(itemId) {
        if (!this.isAuthenticated()) {
            const cart = JSON.parse(localStorage.getItem(CART_KEY)) || [];
            const idx = cart.findIndex(i => i.product_id === itemId || i.id === itemId);
            
            if (idx >= 0) {
                cart.splice(idx, 1);
            }
            
            localStorage.setItem(CART_KEY, JSON.stringify(cart));
            return cart;
        }
        
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/carts/items/${itemId}`, {
            method: 'DELETE'
        });
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.removeCartItem(itemId);
            }
            throw new Error('Failed to remove from cart');
        }
        
        return response.json();
    },
    
    async clearCart() {
        if (!this.isAuthenticated()) {
            localStorage.removeItem(CART_KEY);
            return [];
        }
        
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/carts`, {
            method: 'DELETE'
        });
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.clearCart();
            }
            throw new Error('Failed to clear cart');
        }
        
        return response.json();
    },
    
    // Orders
    async createOrder(orderData) {
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/orders`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(orderData)
        });
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.createOrder(orderData);
            }
            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || 'Failed to create order');
        }
        
        return response.json();
    },
    
    async getOrders() {
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/orders`);
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.getOrders();
            }
            throw new Error('Failed to fetch orders');
        }
        
        const data = await response.json();
        return data.orders || data;
    },
    
    async getOrder(orderId) {
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/orders/${orderId}`);
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.getOrder(orderId);
            }
            throw new Error('Failed to fetch order');
        }
        
        const data = await response.json();
        return data.order || data;
    },
    
    // Payments
    async createPayment(orderId, paymentMethod) {
        const response = await this.authenticatedFetch(`${API_BASE_URL}/v1/payments`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ 
                order_id: orderId, 
                payment_method: paymentMethod,
                amount: paymentMethod.amount
            })
        });
        
        if (!response.ok) {
            if (response.status === 401) {
                await this.refreshToken();
                return this.createPayment(orderId, paymentMethod);
            }
            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || 'Payment failed');
        }
        
        return response.json();
    },
    
    // Helper methods
    isAuthenticated() {
        return !!localStorage.getItem(AUTH_TOKEN_KEY);
    },
    
    getToken() {
        return localStorage.getItem(AUTH_TOKEN_KEY);
    },
    
    getCustomer() {
        return JSON.parse(localStorage.getItem(CUSTOMER_KEY));
    },
    
    setAuth(accessToken, refreshToken) {
        localStorage.setItem(AUTH_TOKEN_KEY, accessToken);
        if (refreshToken) {
            localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
        }
    },
    
    clearAuth() {
        localStorage.removeItem(AUTH_TOKEN_KEY);
        localStorage.removeItem(REFRESH_TOKEN_KEY);
        localStorage.removeItem(CUSTOMER_KEY);
    },
    
    async authenticatedFetch(url, options = {}) {
        const token = this.getToken();
        
        const headers = {
            ...options.headers,
            'Authorization': `Bearer ${token}`
        };
        
        return fetch(url, { ...options, headers });
    },
    
    logout() {
        this.clearAuth();
        localStorage.removeItem(CART_KEY);
        localStorage.removeItem(ORDERS_KEY);
    }
};

// Make API available globally
window.api = api;
