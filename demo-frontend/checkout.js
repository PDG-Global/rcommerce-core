// R Commerce Demo Frontend - Checkout with Stripe

let stripe = null;
let cardElement = null;
let paymentIntentClientSecret = null;

// Initialize checkout page
document.addEventListener('DOMContentLoaded', async () => {
    initCart();
    updateAuthUI();
    loadCheckoutItems();
    
    // Pre-fill customer email if logged in
    const customer = getCustomer();
    if (customer && customer.email) {
        document.getElementById('customerEmail').value = customer.email;
    }
    
    // Initialize Stripe
    await initStripe();
});

// Initialize Stripe Elements
async function initStripe() {
    try {
        // Get Stripe config from API
        const response = await fetch(`${API_BASE_URL}/payments/config`);
        const config = await response.json();
        
        // Initialize Stripe with demo key
        stripe = Stripe(config.publishable_key);
        
        // Create Elements instance
        const elements = stripe.elements({
            appearance: {
                theme: 'stripe',
                variables: {
                    colorPrimary: '#2563eb',
                    colorBackground: '#ffffff',
                    colorText: '#1f2937',
                    colorDanger: '#ef4444',
                    borderRadius: '8px',
                }
            }
        });
        
        // Create and mount Card Element
        cardElement = elements.create('card', {
            style: {
                base: {
                    fontSize: '16px',
                    fontFamily: 'Inter, sans-serif',
                    '::placeholder': {
                        color: '#6b7280'
                    }
                }
            }
        });
        
        cardElement.mount('#cardElement');
        
        // Handle validation errors
        cardElement.on('change', (event) => {
            const errorDiv = document.getElementById('cardErrors');
            if (event.error) {
                errorDiv.textContent = event.error.message;
            } else {
                errorDiv.textContent = '';
            }
        });
        
        // Handle payment submission
        document.getElementById('submitPayment').addEventListener('click', handlePayment);
        
    } catch (error) {
        console.error('Failed to initialize Stripe:', error);
        showMessage('Payment system unavailable', 'error');
    }
}

// Load cart items in checkout
function loadCheckoutItems() {
    const container = document.getElementById('checkoutItems');
    
    if (cart.length === 0) {
        window.location.href = 'cart.html';
        return;
    }
    
    container.innerHTML = cart.map(item => `
        <div class="checkout-item">
            <span class="checkout-item-name">${escape(item.name)}</span>
            <span class="checkout-item-qty">x${item.quantity}</span>
            <span class="checkout-item-price">$${(item.price * item.quantity).toFixed(2)}</span>
        </div>
    `).join('');
    
    const subtotal = cart.reduce((sum, item) => sum + (item.price * item.quantity), 0);
    document.getElementById('subtotal').textContent = `$${subtotal.toFixed(2)}`;
    document.getElementById('total').textContent = `$${subtotal.toFixed(2)}`;
}

// Handle payment submission
async function handlePayment() {
    const submitBtn = document.getElementById('submitPayment');
    const email = document.getElementById('customerEmail').value;
    
    if (!email) {
        showMessage('Please enter your email', 'error');
        return;
    }
    
    submitBtn.disabled = true;
    submitBtn.textContent = 'Processing...';
    
    try {
        // Calculate total
        const total = cart.reduce((sum, item) => sum + (item.price * item.quantity), 0);
        
        // Create payment intent
        const intentResponse = await fetch(`${API_BASE_URL}/payments/intent`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                amount: total,
                currency: 'usd',
                order_id: null
            })
        });
        
        if (!intentResponse.ok) {
            throw new Error('Failed to create payment intent');
        }
        
        const intentData = await intentResponse.json();
        paymentIntentClientSecret = intentData.client_secret;
        
        // Confirm card payment with Stripe
        const { error, paymentIntent } = await stripe.confirmCardPayment(
            paymentIntentClientSecret,
            {
                payment_method: {
                    card: cardElement,
                    billing_details: {
                        email: email
                    }
                }
            }
        );
        
        if (error) {
            throw new Error(error.message);
        }
        
        // Payment succeeded - create order
        await createOrder(paymentIntent.id, email, total);
        
    } catch (error) {
        console.error('Payment failed:', error);
        showMessage(error.message, 'error');
        submitBtn.disabled = false;
        submitBtn.textContent = 'Pay Now';
    }
}

// Create order after successful payment
async function createOrder(paymentIntentId, email, total) {
    try {
        const orderData = {
            items: cart.map(item => ({
                product_id: item.productId,
                quantity: item.quantity,
                price: item.price
            })),
            total: total,
            email: email,
            payment_intent_id: paymentIntentId
        };
        
        // Add auth header if logged in
        const headers = {
            'Content-Type': 'application/json'
        };
        if (isAuthenticated()) {
            headers['Authorization'] = `Bearer ${getToken()}`;
        }
        
        const response = await fetch(`${API_BASE_URL}/orders`, {
            method: 'POST',
            headers: headers,
            body: JSON.stringify(orderData)
        });
        
        let order;
        if (response.ok) {
            order = await response.json();
        } else {
            // Demo mode - create mock order
            order = {
                order: {
                    id: 'ord_' + Math.random().toString(36).substr(2, 9),
                    status: 'confirmed',
                    total: total,
                    created_at: new Date().toISOString()
                }
            };
        }
        
        // Clear cart
        cart = [];
        saveCart();
        updateCartCount();
        
        // Redirect to confirmation page
        const orderId = order.order?.id || order.id;
        window.location.href = `confirmation.html?order_id=${orderId}`;
        
    } catch (error) {
        console.error('Order creation failed:', error);
        // Still redirect to confirmation in demo mode
        window.location.href = `confirmation.html?order_id=demo_${Date.now()}`;
    }
}
