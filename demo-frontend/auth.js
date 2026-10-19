// R Commerce Demo Frontend - Authentication Module
// Uses R Commerce API for customer login/registration

const AUTH_TOKEN_KEY = 'rcommerce_token';
const CUSTOMER_KEY = 'rcommerce_customer';

let currentToken = localStorage.getItem(AUTH_TOKEN_KEY) || null;
let currentCustomer = JSON.parse(localStorage.getItem(CUSTOMER_KEY)) || null;

// Get current auth state
function isAuthenticated() {
    return !!currentToken;
}

function getToken() {
    return currentToken;
}

function getCustomer() {
    return currentCustomer;
}

// Update UI based on auth state
function updateAuthUI() {
    const authSection = document.getElementById('authSection');
    if (!authSection) return;
    
    if (isAuthenticated() && currentCustomer) {
        authSection.innerHTML = `
            <div class="user-menu">
                <span class="user-name">Hello, ${escape(currentCustomer.first_name || 'Customer')}</span>
                <a href="profile.html" class="nav-link">Profile</a>
                <a href="#" class="nav-link" onclick="logout(); return false;">Logout</a>
            </div>
        `;
    } else {
        authSection.innerHTML = `
            <a href="#" class="nav-link" onclick="showLoginModal(); return false;">Login</a>
            <a href="#" class="nav-link" onclick="showRegisterModal(); return false;">Register</a>
        `;
    }
}

// Login with email/password
async function login(email, password) {
    try {
        const response = await fetch(`${API_BASE_URL}/auth/login`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ email, password })
        });
        
        if (!response.ok) {
            throw new Error('Invalid credentials');
        }
        
        const data = await response.json();
        currentToken = data.access_token;
        localStorage.setItem(AUTH_TOKEN_KEY, currentToken);
        
        // Fetch customer profile
        await fetchCustomerProfile();
        
        updateAuthUI();
        hideModal('loginModal');
        showMessage('Logged in successfully!', 'success');
        
        return true;
    } catch (error) {
        showMessage(error.message, 'error');
        return false;
    }
}

// Register new customer
async function register(customerData) {
    try {
        const response = await fetch(`${API_BASE_URL}/auth/register`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(customerData)
        });
        
        if (!response.ok) {
            throw new Error('Registration failed');
        }
        
        const data = await response.json();
        currentCustomer = data.customer;
        localStorage.setItem(CUSTOMER_KEY, JSON.stringify(currentCustomer));
        
        // Auto-login after registration
        await login(customerData.email, customerData.password);
        
        updateAuthUI();
        hideModal('registerModal');
        showMessage('Account created successfully!', 'success');
        
        return true;
    } catch (error) {
        showMessage(error.message, 'error');
        return false;
    }
}

// Fetch customer profile
async function fetchCustomerProfile() {
    try {
        const response = await fetch(`${API_BASE_URL}/customers/me`, {
            headers: { 'Authorization': `Bearer ${currentToken}` }
        });
        
        if (response.ok) {
            const data = await response.json();
            currentCustomer = data.customer || data;
            localStorage.setItem(CUSTOMER_KEY, JSON.stringify(currentCustomer));
        }
    } catch (error) {
        console.error('Failed to fetch profile:', error);
    }
}

// Logout
function logout() {
    currentToken = null;
    currentCustomer = null;
    localStorage.removeItem(AUTH_TOKEN_KEY);
    localStorage.removeItem(CUSTOMER_KEY);
    updateAuthUI();
    showMessage('Logged out successfully', 'success');
}

// Show login modal
function showLoginModal() {
    const modal = document.getElementById('loginModal');
    if (modal) modal.style.display = 'block';
}

// Show register modal
function showRegisterModal() {
    const modal = document.getElementById('registerModal');
    if (modal) modal.style.display = 'block';
}

// Hide modal
function hideModal(modalId) {
    const modal = document.getElementById(modalId);
    if (modal) modal.style.display = 'none';
}

// Close modals when clicking outside
window.onclick = function(event) {
    if (event.target.classList.contains('modal')) {
        event.target.style.display = 'none';
    }
};

// Initialize auth on page load
document.addEventListener('DOMContentLoaded', () => {
    updateAuthUI();
});
