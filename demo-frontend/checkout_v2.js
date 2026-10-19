// R Commerce Demo Frontend - Checkout v2 with Agnostic Payment API
// This version uses the backend API for all payment processing

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
    
    // Setup payment method selection
    setupPaymentMethodSelection();
    
    // Handle payment submission
    document.getElementById('submitPayment').addEventListener('click', handlePayment);
});

// Setup payment method selection (simplified for demo)
function setupPaymentMethodSelection() {
    const container = document.getElementById('paymentMethods');
    const submitBtn = document.getElementById('submitPayment');
    
    // For demo, just show Stripe card option
    container.innerHTML = `
        <div class="payment-method-option">
            <label class="payment-method-label">
                <input type="radio" name="payment_method" value="card" data-gateway="stripe" id="stripe_card" checked>
                <span class="payment-method-icon">💳</span>
                <span class="payment-method-name">Credit/Debit Card (Stripe)</span>
            </label>
        </div>
    `;
    
    // Show card form and enable button
    document.getElementById('cardPaymentForm').style.display = 'block';
    submitBtn.disabled = false;
    
    // Store selection
    window.selectedGateway = 'stripe';
    window.selectedMethod = 'card';
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
            <span class="checkout-item-name">${escapeHtml(item.name)}</span>
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
    
    // Collect card data from form
    const cardNumber = document.getElementById('cardNumber').value.replace(/\s/g, '');
    const expiryMonth = parseInt(document.getElementById('expiryMonth').value);
    const expiryYear = parseInt(document.getElementById('expiryYear').value);
    const cvc = document.getElementById('cvc').value;
    const cardName = document.getElementById('cardName').value;
    
    if (!cardNumber || !expiryMonth || !expiryYear || !cvc) {
        showMessage('Please fill in all card details', 'error');
        return;
    }
    
    submitBtn.disabled = true;
    submitBtn.textContent = 'Processing...';
    
    try {
        // Calculate total
        const total = cart.reduce((sum, item) => sum + (item.price * item.quantity), 0);
        
        // Build payment method data (matches API format)
        const paymentMethodData = {
            type: 'card',
            number: cardNumber,
            exp_month: expiryMonth.toString(),
            exp_year: expiryYear.toString(),
            cvc: cvc,
            name: cardName
        };
        
        const requestBody = {
            gateway_id: 'stripe',
            amount: total.toFixed(2),
            currency: 'usd',
            payment_method_type: 'card',
            payment_method_data: paymentMethodData,
            order_id: 'order_' + Date.now(),
            customer_email: email,
            description: `Order from ${window.location.hostname}`,
            save_payment_method: false
        };
        
        console.log('Sending payment request:', requestBody);
        
        // Create payment via API
        const paymentResponse = await fetch(`${API_BASE_URL}/payments`, {
            method: 'POST',
            headers: { 
                'Content-Type': 'application/json',
                'Accept': 'application/json'
            },
            body: JSON.stringify(requestBody)
        });
        
        console.log('Payment response status:', paymentResponse.status);
        
        if (!paymentResponse.ok) {
            let errorMessage = 'Payment failed';
            try {
                const errorData = await paymentResponse.json();
                errorMessage = errorData.error?.message || errorData.message || `HTTP ${paymentResponse.status}`;
            } catch (e) {
                errorMessage = `Payment failed: HTTP ${paymentResponse.status}`;
            }
            throw new Error(errorMessage);
        }
        
        const paymentResult = await paymentResponse.json();
        console.log('Payment result:', paymentResult);
        
        // Handle different response types - API returns 'result' field
        switch (paymentResult.result) {
            case 'success':
                // Payment succeeded immediately
                showMessage('Payment successful! Creating order...', 'success');
                await createOrder(paymentResult.payment_id, email, total);
                break;
                
            case 'requires_action':
                // Additional action required (3DS, redirect, etc.)
                await handlePaymentAction(paymentResult);
                break;
                
            case 'failed':
                // Payment failed
                throw new Error(paymentResult.error_message || 'Payment failed');
                
            default:
                console.error('Unexpected payment response:', paymentResult);
                throw new Error('Unexpected payment response type: ' + paymentResult.result);
        }
        
    } catch (error) {
        console.error('Payment failed:', error);
        showMessage(error.message, 'error');
        submitBtn.disabled = false;
        submitBtn.textContent = 'Pay Now';
    }
}

// Handle payment that requires additional action
async function handlePaymentAction(paymentResult) {
    const { payment_id, action_type, action_data } = paymentResult;
    
    console.log('Payment requires action:', { action_type, action_data });
    
    switch (action_type) {
        case 'three_d_secure':
            // Handle 3D Secure
            if (action_data?.redirect_url) {
                // Save payment ID for when user returns
                sessionStorage.setItem('pending_payment_id', payment_id);
                // Redirect to 3DS page
                window.location.href = action_data.redirect_url;
            } else {
                showMessage('3D Secure authentication required but no redirect URL provided', 'error');
            }
            break;
            
        case 'redirect':
            // Redirect to payment provider (PayPal, Alipay, etc.)
            if (action_data?.redirect_url) {
                window.location.href = action_data.redirect_url;
            } else {
                showMessage('Redirect required but no URL provided', 'error');
            }
            break;
            
        default:
            showMessage(`Payment action required: ${action_type}`, 'error');
    }
}

// Create order after successful payment
async function createOrder(paymentId, email, total) {
    try {
        // For demo, just show success and redirect
        showMessage('Order created successfully!', 'success');
        
        // Clear cart
        localStorage.removeItem('cart');
        
        // Redirect to confirmation page
        setTimeout(() => {
            window.location.href = `confirmation.html?payment_id=${paymentId}`;
        }, 1500);
        
    } catch (error) {
        console.error('Failed to create order:', error);
        showMessage('Payment successful but order creation failed. Please contact support.', 'error');
    }
}

// Show message to user
function showMessage(message, type = 'info') {
    const messageDiv = document.getElementById('paymentMessage');
    messageDiv.textContent = message;
    messageDiv.className = `payment-message ${type}`;
    messageDiv.style.display = 'block';
    
    // Auto-hide success messages
    if (type === 'success') {
        setTimeout(() => {
            messageDiv.style.display = 'none';
        }, 5000);
    }
}

// Escape HTML to prevent XSS
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}
