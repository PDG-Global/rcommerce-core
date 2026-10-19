// R Commerce Demo Frontend - Main Application
// Single Page Application with client-side routing

// --- Application State ---
const store = {
    user: null,
    cart: [],
    orders: [],
    products: [],
    view: 'home',
    searchQuery: '',
    isLoading: false
};

// --- Main Application ---
const app = {
    init: async () => {
        lucide.createIcons();
        
        // Load user from storage
        store.user = api.getCustomer();
        
        // Load cart
        store.cart = JSON.parse(localStorage.getItem('rc_cart')) || [];
        
        // Setup router
        window.onhashchange = () => {
            const hash = window.location.hash.replace('#', '') || 'home';
            app.router(hash);
        };
        
        // Initial render
        const hash = window.location.hash.replace('#', '') || 'home';
        await app.router(hash);
        
        // Initial telemetry
        app.logAPI('GET', '/v1/store/config', 200, '3ms');
        
        // Refresh icons after render
        lucide.createIcons();
    },

    router: async (view, params = {}) => {
        store.view = view;
        const container = document.getElementById('app');
        container.innerHTML = '';
        window.location.hash = view;
        
        app.setLoading(true);
        
        try {
            switch(view) {
                case 'home':
                    await app.renderHome(container);
                    break;
                case 'cart':
                    await app.renderCart(container);
                    break;
                case 'checkout':
                    await app.renderCheckout(container);
                    break;
                case 'confirmation':
                    app.renderConfirmation(container, params);
                    break;
                case 'login':
                    app.renderLogin(container);
                    break;
                case 'register':
                    app.renderRegister(container);
                    break;
                case 'orders':
                    await app.renderOrders(container);
                    break;
                case 'product':
                    await app.renderProductDetail(container, params.id);
                    break;
                default:
                    await app.renderHome(container);
            }
        } catch (error) {
            console.error('Router error:', error);
            app.renderError(container, error.message);
        }
        
        app.setLoading(false);
        app.updateNav();
        lucide.createIcons();
        window.scrollTo(0, 0);
    },

    // --- Renderers ---
    
    renderHome: async (container) => {
        // Hero Section
        const hero = document.createElement('div');
        hero.className = "mb-12 bg-black text-white rounded-xl p-8 md:p-12 relative overflow-hidden";
        hero.innerHTML = `
            <div class="relative z-10 max-w-2xl">
                <div class="inline-flex items-center gap-2 px-3 py-1 mb-6 text-xs font-mono font-bold text-rust bg-white/10 rounded-full border border-white/10">
                    <span class="w-2 h-2 rounded-full bg-rust animate-pulse"></span>
                    DEMO STORE
                </div>
                <h1 class="text-3xl md:text-5xl font-bold font-mono uppercase tracking-tight mb-4">Speed is a Feature.</h1>
                <p class="text-gray-400 mb-8 max-w-lg">Experience the raw performance of R Commerce. Browse products, add to cart, and checkout in milliseconds.</p>
                <button onclick="document.getElementById('products-grid').scrollIntoView({behavior:'smooth'})" class="bg-white text-black px-6 py-3 rounded-md font-mono text-sm font-bold hover:bg-gray-200 transition-colors btn-press">
                    Shop Collection
                </button>
            </div>
            <div class="absolute right-0 top-0 h-full w-1/3 bg-gradient-to-l from-rust/20 to-transparent"></div>
        `;
        container.appendChild(hero);

        // Load products from API
        try {
            store.products = await api.getProducts();
            app.logAPI('GET', '/v1/products', 200, '12ms');
        } catch (error) {
            app.showToast('Failed to load products', 'error');
            store.products = [];
        }

        // Products Grid
        const gridSection = document.createElement('div');
        gridSection.id = 'products-grid';
        gridSection.innerHTML = `
            <div class="flex justify-between items-center mb-6">
                <h2 class="text-xl font-bold font-mono uppercase">Products</h2>
                <span class="text-sm text-gray-500 font-mono">${store.products.length} items</span>
            </div>
            <div id="products-list" class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-8">
                <!-- Products injected here -->
            </div>
        `;
        container.appendChild(gridSection);
        
        app.renderProductsList(store.products);
    },

    renderProductsList: (products) => {
        const grid = document.getElementById('products-list');
        if (!grid) return;
        
        if (products.length === 0) {
            grid.innerHTML = `
                <div class="col-span-full text-center py-12 text-gray-500">
                    <i data-lucide="package" class="w-12 h-12 mx-auto mb-4 opacity-50"></i>
                    <p>No products available</p>
                </div>
            `;
            return;
        }
        
        grid.innerHTML = products.map(p => {
            const image = p.images?.[0]?.src || p.image || `https://placehold.co/600x400/1F2937/FFF?text=${encodeURIComponent(p.title || p.name)}`;
            const price = parseFloat(p.price).toFixed(2);
            const badge = p.is_featured ? 'Featured' : p.inventory_quantity < 10 ? 'Low Stock' : null;
            
            return `
                <div class="product-card group bg-white border border-gray-200 rounded-lg overflow-hidden hover:shadow-md transition-all cursor-pointer" onclick="app.router('product', {id: '${p.id}'})">
                    <div class="aspect-video bg-gray-100 relative overflow-hidden">
                        <img src="${image}" alt="${p.title || p.name}" class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-500">
                        ${badge ? `<span class="absolute top-2 right-2 bg-black text-white text-[10px] font-mono uppercase px-2 py-1">${badge}</span>` : ''}
                    </div>
                    <div class="p-5">
                        <div class="flex justify-between items-start mb-2">
                            <div>
                                <span class="text-xs text-rust font-mono font-bold uppercase tracking-wider">${p.category || 'Product'}</span>
                                <h3 class="font-bold text-lg mt-1 line-clamp-2">${p.title || p.name}</h3>
                            </div>
                        </div>
                        <p class="text-sm text-gray-500 line-clamp-2 mb-4">${p.description || ''}</p>
                        <div class="flex justify-between items-center mt-4 border-t border-gray-100 pt-4">
                            <span class="font-mono text-lg font-bold">$${price}</span>
                            <button onclick="event.stopPropagation(); app.addToCart('${p.id}', '${p.title || p.name}', ${price})" 
                                    class="bg-black text-white p-2 rounded hover:bg-rust transition-colors flex items-center gap-2 text-sm font-bold px-4 btn-press">
                                <span>Add</span>
                                <i data-lucide="plus" class="w-4 h-4"></i>
                            </button>
                        </div>
                    </div>
                </div>
            `;
        }).join('');
        
        lucide.createIcons();
    },

    renderProductDetail: async (container, productId) => {
        try {
            const product = await api.getProduct(productId);
            app.logAPI('GET', `/v1/products/${productId}`, 200, '8ms');
            
            const image = product.images?.[0]?.src || product.image || `https://placehold.co/800x600/1F2937/FFF?text=${encodeURIComponent(product.title || product.name)}`;
            const price = parseFloat(product.price).toFixed(2);
            
            container.innerHTML = `
                <div class="mb-4">
                    <button onclick="app.router('home')" class="text-sm text-gray-500 hover:text-black flex items-center gap-1">
                        <i data-lucide="arrow-left" class="w-4 h-4"></i> Back to products
                    </button>
                </div>
                
                <div class="grid md:grid-cols-2 gap-8 bg-white rounded-xl border border-gray-200 overflow-hidden">
                    <div class="aspect-square bg-gray-100">
                        <img src="${image}" alt="${product.title || product.name}" class="w-full h-full object-cover">
                    </div>
                    <div class="p-8 flex flex-col">
                        <span class="text-xs text-rust font-mono font-bold uppercase tracking-wider mb-2">${product.category || 'Product'}</span>
                        <h1 class="text-3xl font-bold mb-4">${product.title || product.name}</h1>
                        <p class="text-gray-600 mb-6 flex-grow">${product.description || 'No description available.'}</p>
                        
                        <div class="border-t border-gray-100 pt-6">
                            <div class="flex items-center justify-between mb-6">
                                <span class="font-mono text-3xl font-bold">$${price}</span>
                                ${product.inventory_quantity !== undefined ? `
                                    <span class="text-sm ${product.inventory_quantity < 10 ? 'text-red-600' : 'text-green-600'} font-mono">
                                        ${product.inventory_quantity} in stock
                                    </span>
                                ` : ''}
                            </div>
                            
                            <button onclick="app.addToCart('${product.id}', '${product.title || product.name}', ${price})" 
                                    class="w-full bg-black text-white py-4 rounded-md font-bold hover:bg-rust transition-colors flex justify-center items-center gap-2 btn-press">
                                <i data-lucide="shopping-bag" class="w-5 h-5"></i>
                                Add to Cart
                            </button>
                        </div>
                    </div>
                </div>
            `;
        } catch (error) {
            app.renderError(container, 'Product not found');
        }
    },

    renderCart: async (container) => {
        // Merge local cart with API cart if authenticated
        if (api.isAuthenticated()) {
            try {
                const apiCart = await api.getCart();
                // TODO: Merge carts properly
            } catch (error) {
                console.error('Failed to load cart:', error);
            }
        }
        
        // Enrich cart with product details
        const enrichedCart = store.cart.map(item => {
            const product = store.products.find(p => p.id === item.productId) || {};
            return {
                ...item,
                product: {
                    ...product,
                    name: item.name || product.title || product.name || 'Unknown Product',
                    image: product.images?.[0]?.src || product.image || `https://placehold.co/200x200/1F2937/FFF?text=${encodeURIComponent(item.name || 'Product')}`,
                    price: item.price || product.price || 0
                }
            };
        });
        
        if (enrichedCart.length === 0) {
            container.innerHTML = `
                <div class="text-center py-24">
                    <div class="bg-gray-100 w-16 h-16 rounded-full flex items-center justify-center mx-auto mb-6">
                        <i data-lucide="shopping-cart" class="w-8 h-8 text-gray-400"></i>
                    </div>
                    <h2 class="text-2xl font-bold mb-2">Your cart is empty</h2>
                    <p class="text-gray-500 mb-8">Looks like you haven't added anything yet.</p>
                    <button onclick="app.router('home')" class="bg-black text-white px-6 py-3 rounded-md font-mono text-sm font-bold hover:bg-gray-800 btn-press">
                        Start Shopping
                    </button>
                </div>
            `;
            return;
        }

        const total = enrichedCart.reduce((sum, item) => sum + (item.product.price * item.quantity), 0);
        
        container.innerHTML = `
            <h1 class="text-2xl font-bold font-mono mb-8 border-b border-gray-200 pb-4 uppercase">Shopping Cart (${enrichedCart.length})</h1>
            <div class="flex flex-col lg:flex-row gap-12">
                <div class="flex-1 space-y-4">
                    ${enrichedCart.map((item, idx) => `
                        <div class="flex gap-4 p-4 bg-white border border-gray-200 rounded-lg items-center">
                            <img src="${item.product.image}" class="w-20 h-20 object-cover rounded bg-gray-100 cursor-pointer" onclick="app.router('product', {id: '${item.productId}'})">
                            <div class="flex-1">
                                <h3 class="font-bold cursor-pointer hover:text-rust" onclick="app.router('product', {id: '${item.productId}'})">${item.product.name}</h3>
                                <p class="text-sm text-gray-500 font-mono">$${parseFloat(item.product.price).toFixed(2)} each</p>
                            </div>
                            <div class="flex items-center gap-4">
                                <div class="flex items-center border border-gray-200 rounded">
                                    <button onclick="app.updateQty(${idx}, -1)" class="px-3 py-1 hover:bg-gray-50 btn-press">-</button>
                                    <span class="px-2 font-mono text-sm min-w-[2rem] text-center">${item.quantity}</span>
                                    <button onclick="app.updateQty(${idx}, 1)" class="px-3 py-1 hover:bg-gray-50 btn-press">+</button>
                                </div>
                                <button onclick="app.removeFromCart(${idx})" class="text-gray-400 hover:text-red-600 btn-press">
                                    <i data-lucide="trash-2" class="w-5 h-5"></i>
                                </button>
                            </div>
                        </div>
                    `).join('')}
                </div>
                <div class="w-full lg:w-96">
                    <div class="bg-white p-6 rounded-lg border border-gray-200 shadow-sm sticky top-24">
                        <h3 class="font-bold text-lg mb-4">Summary</h3>
                        <div class="space-y-2 text-sm mb-6 border-b border-gray-100 pb-6">
                            <div class="flex justify-between">
                                <span class="text-gray-600">Subtotal</span>
                                <span class="font-mono">$${total.toFixed(2)}</span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-gray-600">Shipping</span>
                                <span class="font-mono text-green-600">Free</span>
                            </div>
                            <div class="flex justify-between">
                                <span class="text-gray-600">Tax</span>
                                <span class="font-mono">Calculated at checkout</span>
                            </div>
                        </div>
                        <div class="flex justify-between items-center mb-6">
                            <span class="font-bold">Total</span>
                            <span class="font-bold font-mono text-xl">$${total.toFixed(2)}</span>
                        </div>
                        <button onclick="app.router('checkout')" class="w-full bg-rust text-white py-3 rounded-md font-bold hover:bg-[#c2410c] transition-colors flex justify-center items-center gap-2 btn-press">
                            Checkout <i data-lucide="arrow-right" class="w-4 h-4"></i>
                        </button>
                    </div>
                </div>
            </div>
        `;
        
        app.logAPI('GET', '/v1/cart', 200, '5ms');
    },

    renderCheckout: async (container) => {
        if (!api.isAuthenticated()) {
            // Save intended destination
            sessionStorage.setItem('redirectAfterLogin', 'checkout');
            app.router('login');
            return;
        }
        
        if (store.cart.length === 0) {
            app.router('cart');
            return;
        }
        
        const total = store.cart.reduce((sum, item) => sum + (item.price * item.quantity), 0);
        
        container.innerHTML = `
            <div class="max-w-2xl mx-auto">
                <div class="flex items-center gap-2 mb-8 text-sm text-gray-500 font-mono">
                    <span class="cursor-pointer hover:text-black" onclick="app.router('cart')">Cart</span>
                    <i data-lucide="chevron-right" class="w-4 h-4"></i>
                    <span class="text-black font-bold">Checkout</span>
                </div>
                
                <div class="bg-white p-8 border border-gray-200 rounded-xl shadow-sm">
                    <h2 class="text-2xl font-bold mb-6 font-mono uppercase">Checkout</h2>
                    <form onsubmit="app.processPayment(event)">
                        <div class="space-y-6">
                            <div>
                                <label class="block text-xs font-bold uppercase text-gray-500 mb-2">Contact Information</label>
                                <input type="email" id="checkout-email" value="${store.user?.email || ''}" required 
                                       class="w-full border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                       placeholder="Email">
                            </div>
                            
                            <div>
                                <label class="block text-xs font-bold uppercase text-gray-500 mb-2">Shipping Address</label>
                                <div class="grid grid-cols-2 gap-4">
                                    <input required type="text" id="shipping-first" placeholder="First Name" 
                                           class="border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                           value="${store.user?.first_name || ''}">
                                    <input required type="text" id="shipping-last" placeholder="Last Name" 
                                           class="border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                           value="${store.user?.last_name || ''}">
                                    <input required type="text" id="shipping-address" placeholder="Street Address" 
                                           class="col-span-2 border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent">
                                    <input required type="text" id="shipping-city" placeholder="City" 
                                           class="border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent">
                                    <input required type="text" id="shipping-zip" placeholder="ZIP Code" 
                                           class="border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent">
                                </div>
                            </div>
                            
                            <div>
                                <label class="block text-xs font-bold uppercase text-gray-500 mb-2">Payment Method</label>
                                <div class="space-y-2">
                                    <label class="border border-gray-200 rounded p-4 flex items-center gap-3 cursor-pointer hover:bg-gray-50 transition-colors">
                                        <input type="radio" name="payment" value="card" checked class="text-rust focus:ring-rust">
                                        <i data-lucide="credit-card" class="w-5 h-5 text-gray-600"></i>
                                        <span class="text-sm">Credit Card (Demo)</span>
                                        <span class="ml-auto text-xs bg-gray-100 text-gray-600 px-2 py-1 rounded font-mono">4242</span>
                                    </label>
                                    <label class="border border-gray-200 rounded p-4 flex items-center gap-3 cursor-pointer hover:bg-gray-50 transition-colors opacity-50">
                                        <input type="radio" name="payment" value="stripe" disabled class="text-rust">
                                        <i data-lucide="stripe" class="w-5 h-5 text-gray-600"></i>
                                        <span class="text-sm">Stripe (Coming Soon)</span>
                                    </label>
                                </div>
                            </div>
                            
                            <div class="bg-gray-50 p-4 rounded-lg">
                                <h4 class="font-bold text-sm mb-3">Order Summary</h4>
                                <div class="space-y-2 text-sm">
                                    ${store.cart.map(item => `
                                        <div class="flex justify-between">
                                            <span class="text-gray-600">${item.name} x${item.quantity}</span>
                                            <span class="font-mono">$${(item.price * item.quantity).toFixed(2)}</span>
                                        </div>
                                    `).join('')}
                                </div>
                                <div class="border-t border-gray-200 mt-3 pt-3 flex justify-between font-bold">
                                    <span>Total</span>
                                    <span class="font-mono text-xl">$${total.toFixed(2)}</span>
                                </div>
                            </div>
                            
                            <button type="submit" id="pay-btn" class="w-full bg-black text-white py-4 rounded-md font-bold hover:bg-gray-800 transition-colors flex justify-center items-center gap-2 btn-press">
                                <span>Pay $${total.toFixed(2)}</span>
                            </button>
                        </div>
                    </form>
                </div>
            </div>
        `;
    },

    renderConfirmation: (container, params = {}) => {
        const order = params.order || JSON.parse(sessionStorage.getItem('lastOrder') || '{}');
        
        if (!order.id) {
            app.router('home');
            return;
        }
        
        container.innerHTML = `
            <div class="max-w-2xl mx-auto text-center pt-12">
                <div class="w-20 h-20 bg-green-100 rounded-full flex items-center justify-center mx-auto mb-6">
                    <i data-lucide="check" class="w-10 h-10 text-green-600"></i>
                </div>
                <h1 class="text-3xl font-bold mb-2">Order Confirmed!</h1>
                <p class="text-gray-500 mb-8">Thank you for your purchase. We've sent a confirmation to ${store.user?.email || 'your email'}.</p>
                
                <div class="bg-white border border-gray-200 rounded-lg p-6 text-left mb-8">
                    <div class="flex justify-between mb-4 border-b border-gray-100 pb-4">
                        <span class="text-gray-500 text-sm">Order Number</span>
                        <span class="font-mono font-bold text-rust">#${order.id}</span>
                    </div>
                    <div class="space-y-2">
                        ${order.items?.map(i => `
                            <div class="flex justify-between text-sm">
                                <span>${i.name} x${i.quantity}</span>
                                <span class="font-mono">$${(i.price * i.quantity).toFixed(2)}</span>
                            </div>
                        `).join('') || ''}
                    </div>
                    <div class="flex justify-between mt-4 pt-4 border-t border-gray-100 font-bold">
                        <span>Total</span>
                        <span class="font-mono">$${order.total?.toFixed(2) || '0.00'}</span>
                    </div>
                </div>
                
                <div class="flex gap-4 justify-center">
                    <button onclick="app.router('orders')" class="bg-black text-white px-6 py-3 rounded-md font-bold hover:bg-gray-800 transition-colors btn-press">
                        View Orders
                    </button>
                    <button onclick="app.router('home')" class="text-rust hover:text-black font-bold text-sm py-3 px-6">
                        Continue Shopping →
                    </button>
                </div>
            </div>
        `;
    },

    renderLogin: (container) => {
        if (api.isAuthenticated()) {
            app.router('home');
            return;
        }
        
        container.innerHTML = `
            <div class="max-w-md mx-auto py-12">
                <div class="bg-white p-8 border border-gray-200 rounded-xl shadow-sm">
                    <h2 class="text-2xl font-bold mb-1 font-mono uppercase">Sign In</h2>
                    <p class="text-sm text-gray-500 mb-6">Welcome back to R Commerce</p>
                    
                    <form onsubmit="app.handleLogin(event)" class="space-y-4">
                        <div>
                            <label class="block text-xs font-bold uppercase text-gray-500 mb-1">Email</label>
                            <input type="email" id="login-email" required 
                                   class="w-full border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                   placeholder="you@example.com">
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase text-gray-500 mb-1">Password</label>
                            <input type="password" id="login-password" required 
                                   class="w-full border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                   placeholder="••••••••">
                        </div>
                        <button type="submit" id="login-btn" class="w-full bg-black text-white py-3 rounded-md font-bold hover:bg-gray-800 transition-colors btn-press">
                            Sign In
                        </button>
                    </form>
                    
                    <div class="mt-6 text-center">
                        <p class="text-sm text-gray-500">
                            Don't have an account? 
                            <a href="#" onclick="app.router('register'); return false;" class="text-rust hover:underline font-bold">Create one</a>
                        </p>
                    </div>
                    
                    <div class="mt-6 pt-6 border-t border-gray-100">
                        <p class="text-xs text-gray-400 text-center">Demo credentials: demo@example.com / demo123</p>
                    </div>
                </div>
            </div>
        `;
    },

    renderRegister: (container) => {
        if (api.isAuthenticated()) {
            app.router('home');
            return;
        }
        
        container.innerHTML = `
            <div class="max-w-md mx-auto py-12">
                <div class="bg-white p-8 border border-gray-200 rounded-xl shadow-sm">
                    <h2 class="text-2xl font-bold mb-1 font-mono uppercase">Create Account</h2>
                    <p class="text-sm text-gray-500 mb-6">Join R Commerce today</p>
                    
                    <form onsubmit="app.handleRegister(event)" class="space-y-4">
                        <div class="grid grid-cols-2 gap-4">
                            <div>
                                <label class="block text-xs font-bold uppercase text-gray-500 mb-1">First Name</label>
                                <input type="text" id="reg-first" required 
                                       class="w-full border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                       placeholder="John">
                            </div>
                            <div>
                                <label class="block text-xs font-bold uppercase text-gray-500 mb-1">Last Name</label>
                                <input type="text" id="reg-last" required 
                                       class="w-full border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                       placeholder="Doe">
                            </div>
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase text-gray-500 mb-1">Email</label>
                            <input type="email" id="reg-email" required 
                                   class="w-full border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                   placeholder="you@example.com">
                        </div>
                        <div>
                            <label class="block text-xs font-bold uppercase text-gray-500 mb-1">Password</label>
                            <input type="password" id="reg-password" required minlength="8"
                                   class="w-full border border-gray-200 rounded p-3 text-sm focus:ring-2 focus:ring-rust focus:border-transparent"
                                   placeholder="••••••••">
                            <p class="text-xs text-gray-400 mt-1">Must be at least 8 characters</p>
                        </div>
                        <button type="submit" id="reg-btn" class="w-full bg-black text-white py-3 rounded-md font-bold hover:bg-gray-800 transition-colors btn-press">
                            Create Account
                        </button>
                    </form>
                    
                    <div class="mt-6 text-center">
                        <p class="text-sm text-gray-500">
                            Already have an account? 
                            <a href="#" onclick="app.router('login'); return false;" class="text-rust hover:underline font-bold">Sign in</a>
                        </p>
                    </div>
                </div>
            </div>
        `;
    },

    renderOrders: async (container) => {
        if (!api.isAuthenticated()) {
            sessionStorage.setItem('redirectAfterLogin', 'orders');
            app.router('login');
            return;
        }
        
        container.innerHTML = `
            <h1 class="text-2xl font-bold font-mono mb-8 border-b border-gray-200 pb-4 uppercase">Order History</h1>
            <div id="orders-list" class="space-y-4">
                <div class="text-center py-12">
                    <div class="loader mx-auto"></div>
                    <p class="text-gray-500 mt-4">Loading orders...</p>
                </div>
            </div>
        `;
        
        try {
            const orders = await api.getOrders();
            store.orders = orders;
            app.logAPI('GET', '/v1/orders', 200, '40ms');
            
            const list = document.getElementById('orders-list');
            
            if (orders.length === 0) {
                list.innerHTML = `
                    <div class="text-center py-12 text-gray-500">
                        <i data-lucide="package" class="w-12 h-12 mx-auto mb-4 opacity-50"></i>
                        <p>No orders found.</p>
                    </div>
                `;
                return;
            }
            
            list.innerHTML = orders.map(o => `
                <div class="bg-white border border-gray-200 rounded-lg p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4 hover:bg-gray-50 transition-colors">
                    <div>
                        <div class="font-mono font-bold text-rust mb-1">#${o.id || o.order_number}</div>
                        <div class="text-xs text-gray-500">${new Date(o.created_at || o.date).toLocaleDateString()} • ${o.items?.length || 0} items</div>
                    </div>
                    <div class="flex items-center gap-4">
                        <span class="px-2 py-1 bg-green-100 text-green-700 text-xs font-bold rounded uppercase">${o.status || 'Paid'}</span>
                        <span class="font-mono font-bold">$${parseFloat(o.total || 0).toFixed(2)}</span>
                        <button class="text-gray-400 hover:text-black btn-press">
                            <i data-lucide="chevron-right" class="w-5 h-5"></i>
                        </button>
                    </div>
                </div>
            `).join('');
            
            lucide.createIcons();
        } catch (error) {
            document.getElementById('orders-list').innerHTML = `
                <div class="text-center py-12 text-gray-500">
                    <p>Failed to load orders.</p>
                    <button onclick="app.router('orders')" class="text-rust hover:underline mt-2">Try again</button>
                </div>
            `;
        }
    },

    renderError: (container, message) => {
        container.innerHTML = `
            <div class="text-center py-24">
                <div class="bg-red-100 w-16 h-16 rounded-full flex items-center justify-center mx-auto mb-6">
                    <i data-lucide="alert-circle" class="w-8 h-8 text-red-600"></i>
                </div>
                <h2 class="text-2xl font-bold mb-2">Something went wrong</h2>
                <p class="text-gray-500 mb-8">${message}</p>
                <button onclick="app.router('home')" class="bg-black text-white px-6 py-3 rounded-md font-mono text-sm font-bold hover:bg-gray-800 btn-press">
                    Go Home
                </button>
            </div>
        `;
    },

    // --- Actions ---
    
    handleSearch: (e) => {
        const term = e.target.value.toLowerCase();
        
        if (store.view !== 'home') {
            app.router('home');
            setTimeout(() => {
                document.getElementById('search-input').value = term;
                app.handleSearch({ target: { value: term } });
            }, 100);
            return;
        }
        
        const filtered = store.products.filter(p => 
            (p.title || p.name || '').toLowerCase().includes(term) ||
            (p.description || '').toLowerCase().includes(term)
        );
        
        app.renderProductsList(filtered);
        
        if (term.length > 2 && !app.searchDebounce) {
            app.logAPI('POST', '/v1/search', 200, '8ms');
            app.searchDebounce = setTimeout(() => app.searchDebounce = null, 1000);
        }
    },

    addToCart: async (id, name, price) => {
        const existing = store.cart.find(i => i.productId === id);
        
        if (existing) {
            existing.quantity++;
        } else {
            store.cart.push({ productId: id, name, price: parseFloat(price), quantity: 1 });
        }
        
        localStorage.setItem('rc_cart', JSON.stringify(store.cart));
        app.updateNav();
        app.showToast(`Added ${name} to cart`);
        app.logAPI('POST', '/v1/carts/items', 201, '14ms');
        
        // Sync with API if authenticated
        if (api.isAuthenticated()) {
            try {
                await api.addToCart(id, 1);
            } catch (error) {
                console.error('Failed to sync cart:', error);
            }
        }
    },

    removeFromCart: (idx) => {
        const item = store.cart[idx];
        store.cart.splice(idx, 1);
        localStorage.setItem('rc_cart', JSON.stringify(store.cart));
        app.updateNav();
        app.renderCart(document.getElementById('app'));
        app.showToast('Removed from cart');
        app.logAPI('DELETE', `/v1/carts/items/${idx}`, 204, '11ms');
    },

    updateQty: (idx, change) => {
        store.cart[idx].quantity += change;
        if (store.cart[idx].quantity < 1) {
            app.removeFromCart(idx);
            return;
        }
        localStorage.setItem('rc_cart', JSON.stringify(store.cart));
        app.updateNav();
        app.renderCart(document.getElementById('app'));
    },

    handleLogin: async (e) => {
        e.preventDefault();
        const btn = document.getElementById('login-btn');
        btn.disabled = true;
        btn.innerHTML = '<div class="loader mx-auto"></div>';
        
        try {
            const email = document.getElementById('login-email').value;
            const password = document.getElementById('login-password').value;
            
            await api.login(email, password);
            store.user = api.getCustomer();
            
            app.showToast('Welcome back!', 'success');
            app.updateNav();
            
            const redirect = sessionStorage.getItem('redirectAfterLogin') || 'home';
            sessionStorage.removeItem('redirectAfterLogin');
            app.router(redirect);
        } catch (error) {
            btn.disabled = false;
            btn.innerHTML = 'Sign In';
            app.showToast(error.message, 'error');
        }
    },

    handleRegister: async (e) => {
        e.preventDefault();
        const btn = document.getElementById('reg-btn');
        btn.disabled = true;
        btn.innerHTML = '<div class="loader mx-auto"></div>';
        
        try {
            const customerData = {
                first_name: document.getElementById('reg-first').value,
                last_name: document.getElementById('reg-last').value,
                email: document.getElementById('reg-email').value,
                password: document.getElementById('reg-password').value,
                accepts_marketing: false,
                currency: 'USD'
            };
            
            await api.register(customerData);
            store.user = api.getCustomer();
            
            app.showToast('Account created!', 'success');
            app.updateNav();
            app.router('home');
        } catch (error) {
            btn.disabled = false;
            btn.innerHTML = 'Create Account';
            app.showToast(error.message, 'error');
        }
    },

    logout: () => {
        api.logout();
        store.user = null;
        store.cart = [];
        localStorage.removeItem('rc_cart');
        app.updateNav();
        app.showToast('Logged out', 'success');
        app.router('home');
    },

    processPayment: async (e) => {
        e.preventDefault();
        const btn = document.getElementById('pay-btn');
        const originalText = btn.innerHTML;
        
        btn.disabled = true;
        btn.innerHTML = `<div class="loader mr-2"></div> Processing...`;
        
        try {
            app.logAPI('POST', '/v1/orders', 202, 'Processing...');
            
            // Create order
            const orderData = {
                items: store.cart.map(i => ({
                    product_id: i.productId,
                    quantity: i.quantity,
                    price: i.price
                })),
                total: store.cart.reduce((sum, i) => sum + (i.price * i.quantity), 0),
                shipping_address: {
                    first_name: document.getElementById('shipping-first').value,
                    last_name: document.getElementById('shipping-last').value,
                    address1: document.getElementById('shipping-address').value,
                    city: document.getElementById('shipping-city').value,
                    zip: document.getElementById('shipping-zip').value,
                    country: 'US'
                }
            };
            
            const order = await api.createOrder(orderData);
            app.logAPI('POST', '/v1/payments', 200, '350ms');
            
            // Clear cart
            store.cart = [];
            localStorage.removeItem('rc_cart');
            app.updateNav();
            
            // Store order for confirmation page
            sessionStorage.setItem('lastOrder', JSON.stringify({
                id: order.id || order.order_id || 'ORD-' + Date.now(),
                items: orderData.items,
                total: orderData.total
            }));
            
            app.router('confirmation');
        } catch (error) {
            btn.disabled = false;
            btn.innerHTML = originalText;
            app.showToast(error.message, 'error');
        }
    },

    // --- Utilities ---
    
    updateNav: () => {
        // Cart Badge
        const count = store.cart.reduce((sum, i) => sum + i.quantity, 0);
        const badge = document.getElementById('cart-count');
        badge.textContent = count;
        badge.style.display = count > 0 ? 'block' : 'none';

        // User Menu
        const userMenu = document.getElementById('user-menu');
        if (store.user) {
            userMenu.innerHTML = `
                <button class="flex items-center gap-2 p-2 hover:bg-gray-100 rounded-md transition-colors font-mono text-xs font-bold">
                    <span class="bg-gray-200 w-6 h-6 flex items-center justify-center rounded-full text-gray-600">${store.user.first_name?.[0] || store.user.email?.[0] || 'U'}</span>
                    <span class="hidden sm:block">${store.user.first_name || store.user.email?.split('@')[0] || 'User'}</span>
                    <i data-lucide="chevron-down" class="w-3 h-3"></i>
                </button>
                <div class="absolute right-0 top-full mt-2 w-48 bg-white border border-gray-200 rounded-md shadow-lg py-1 hidden group-hover:block z-50">
                    <button onclick="app.router('orders')" class="block w-full text-left px-4 py-2 text-sm hover:bg-gray-50 flex items-center gap-2">
                        <i data-lucide="package" class="w-4 h-4"></i> Orders
                    </button>
                    <div class="border-t border-gray-100 my-1"></div>
                    <button onclick="app.logout()" class="block w-full text-left px-4 py-2 text-sm hover:bg-gray-50 text-red-600 flex items-center gap-2">
                        <i data-lucide="log-out" class="w-4 h-4"></i> Sign Out
                    </button>
                </div>
            `;
        } else {
            userMenu.innerHTML = `
                <button onclick="app.router('login')" class="p-2 hover:bg-gray-100 rounded-md transition-colors text-sm font-bold flex items-center gap-2">
                    <i data-lucide="user" class="w-4 h-4"></i>
                    <span class="hidden sm:block">Sign In</span>
                </button>
            `;
        }
        
        lucide.createIcons();
    },

    logAPI: (method, endpoint, status, time) => {
        const telemetry = document.getElementById('telemetry');
        if (!telemetry) return;
        
        const log = document.createElement('div');
        
        let color = 'text-green-500';
        if (status >= 400) color = 'text-red-500';
        else if (status >= 300) color = 'text-yellow-500';
        else if (status === 202) color = 'text-blue-500';

        log.className = "bg-black/90 backdrop-blur text-white p-3 rounded-md text-[10px] font-mono shadow-lg border border-white/10 w-full animate-slide-up flex justify-between items-center pointer-events-auto";
        log.innerHTML = `
            <div class="flex gap-2">
                <span class="font-bold text-gray-400">${method}</span>
                <span class="truncate max-w-[120px]">${endpoint}</span>
            </div>
            <div class="flex gap-3">
                <span class="${color}">${status}</span>
                <span class="text-gray-500">${time}</span>
            </div>
        `;
        
        telemetry.appendChild(log);
        
        setTimeout(() => {
            log.style.opacity = '0';
            log.style.transform = 'translateY(10px)';
            setTimeout(() => log.remove(), 300);
        }, 4000);
    },

    showToast: (message, type = 'success') => {
        const container = document.getElementById('toastContainer');
        if (!container) return;
        
        const toast = document.createElement('div');
        toast.className = `toast ${type}`;
        
        const icon = type === 'success' ? 'check-circle' : type === 'error' ? 'alert-circle' : 'info';
        
        toast.innerHTML = `
            <div class="flex items-center gap-3">
                <i data-lucide="${icon}" class="w-5 h-5 ${type === 'success' ? 'text-green-500' : type === 'error' ? 'text-red-500' : 'text-blue-500'}"></i>
                <span class="text-sm font-medium">${message}</span>
            </div>
        `;
        
        container.appendChild(toast);
        lucide.createIcons();
        
        setTimeout(() => {
            toast.style.opacity = '0';
            toast.style.transform = 'translateX(100%)';
            setTimeout(() => toast.remove(), 300);
        }, 3000);
    },

    setLoading: (loading) => {
        store.isLoading = loading;
        // Could add global loading indicator here
    }
};

// Boot
document.addEventListener('DOMContentLoaded', app.init);

// Make app available globally
window.app = app;
