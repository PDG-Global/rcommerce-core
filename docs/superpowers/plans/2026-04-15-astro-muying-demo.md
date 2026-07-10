# MUYING Coffee Astro Demo Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an Astro SSR frontend for `demo-muying/` that replicates the MUYING Coffee brand experience and is fully wired to the R Commerce API, deployed on Cloudflare Workers.

**Architecture:** Astro with `output: 'server'` and `@astrojs/cloudflare` adapter. All API calls go through a server-side proxy (`/api/v1/*`) to keep secrets secure. Client islands handle cart, checkout, and auth interactions.

**Tech Stack:** Astro 5, Tailwind CSS 3, TypeScript, `@astrojs/cloudflare`, Wrangler 4

---

## File Structure

```
demo-muying/
├── astro.config.mjs
├── package.json
├── tsconfig.json
├── wrangler.toml
├── tailwind.config.mjs
├── src/
│   ├── env.d.ts
│   ├── layouts/
│   │   └── Layout.astro
│   ├── pages/
│   │   ├── index.astro
│   │   ├── products/
│   │   │   └── [id].astro
│   │   ├── guides/
│   │   │   └── [slug].astro
│   │   ├── blog/
│   │   │   └── [slug].astro
│   │   └── api/
│   │       └── v1/
│   │           └── [...path].ts
│   ├── components/
│   │   ├── Nav.astro
│   │   ├── Hero.astro
│   │   ├── About.astro
│   │   ├── AppSection.astro
│   │   ├── ShopSection.astro
│   │   ├── ProductCard.astro
│   │   ├── GuidesSection.astro
│   │   ├── BlogSection.astro
│   │   ├── Footer.astro
│   │   ├── CartDrawer.tsx
│   │   ├── ProductOverlay.tsx
│   │   ├── CheckoutForm.tsx
│   │   └── AuthModal.tsx
│   ├── lib/
│   │   ├── api.ts
│   │   ├── cart.ts
│   │   └── types.ts
│   └── styles/
│       └── global.css
└── public/
    └── (empty for now — using Unsplash images)
```

---

### Task 1: Scaffold Astro Project

**Files:**
- Create: `demo-muying/package.json`
- Create: `demo-muying/astro.config.mjs`
- Create: `demo-muying/tsconfig.json`
- Create: `demo-muying/tailwind.config.mjs`
- Create: `demo-muying/wrangler.toml`
- Create: `demo-muying/src/env.d.ts`

- [ ] **Step 1: Write package.json**

```json
{
  "name": "muying-demo",
  "type": "module",
  "version": "0.1.0",
  "scripts": {
    "dev": "astro dev",
    "build": "astro build",
    "preview": "astro preview",
    "deploy": "wrangler deploy"
  },
  "dependencies": {
    "@astrojs/cloudflare": "^12.0.0",
    "@astrojs/tailwind": "^6.0.0",
    "astro": "^5.0.0",
    "lucide-react": "^0.400.0",
    "tailwindcss": "^3.4.0"
  },
  "devDependencies": {
    "@types/node": "^20.0.0",
    "wrangler": "^4.0.0"
  }
}
```

- [ ] **Step 2: Write astro.config.mjs**

```js
import { defineConfig } from 'astro/config';
import cloudflare from '@astrojs/cloudflare';
import tailwind from '@astrojs/tailwind';

export default defineConfig({
  output: 'server',
  adapter: cloudflare({
    mode: 'directory',
    functionPerRoute: false,
  }),
  integrations: [tailwind()],
  security: {
    checkOrigin: true,
  },
  vite: {
    build: {
      minify: true,
    },
  },
});
```

- [ ] **Step 3: Write tailwind.config.mjs**

```js
/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,ts,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'sans-serif'],
        mono: ['Space Mono', 'monospace'],
        chinese: ['Ma Shan Zheng', 'cursive'],
      },
      colors: {
        muying: {
          50: '#FAFAF9',
          100: '#F5F5F4',
          300: '#D6D3CD',
          500: '#78716c',
          700: '#57534e',
          800: '#292524',
          900: '#1c1917',
          brown: '#3b3024',
          green: '#354f48',
          accent: '#a89f91',
        },
      },
    },
  },
  plugins: [],
};
```

- [ ] **Step 4: Write wrangler.toml**

```toml
name = "muying-demo"
compatibility_date = "2024-01-01"

[vars]
RCOMMERCE_API_URL = "http://localhost:8080"
```

- [ ] **Step 5: Write tsconfig.json**

```json
{
  "extends": "astro/tsconfigs/strict",
  "compilerOptions": {
    "jsx": "react-jsx",
    "jsxImportSource": "react"
  }
}
```

- [ ] **Step 6: Write src/env.d.ts**

```typescript
/// <reference path="../.astro/types.d.ts" />
/// <reference types="@astrojs/cloudflare" />
```

- [ ] **Step 7: Install dependencies**

Run:
```bash
cd demo-muying && npm install
```

Expected: `node_modules/` created, no errors.

- [ ] **Step 8: Commit**

```bash
git add demo-muying/package.json demo-muying/package-lock.json demo-muying/astro.config.mjs demo-muying/tsconfig.json demo-muying/tailwind.config.mjs demo-muying/wrangler.toml demo-muying/src/env.d.ts
git commit -m "chore: scaffold Astro project for muying demo"
```

---

### Task 2: API Proxy and Helpers

**Files:**
- Create: `demo-muying/src/pages/api/v1/[...path].ts`
- Create: `demo-muying/src/lib/api.ts`
- Create: `demo-muying/src/lib/types.ts`

- [ ] **Step 1: Write API proxy route**

`src/pages/api/v1/[...path].ts`:
```typescript
import type { APIRoute } from 'astro';

export const ALL: APIRoute = async ({ request, params, locals }) => {
  const path = params.path || '';
  const url = new URL(request.url);
  const search = url.search;

  const runtime = locals.runtime;
  const apiUrl = runtime?.env?.RCOMMERCE_API_URL || 'http://localhost:8080';
  const apiKey = runtime?.env?.RCOMMERCE_API_KEY;

  const targetUrl = `${apiUrl}/api/v1/${path}${search}`;

  const headers = new Headers();
  const contentType = request.headers.get('content-type');
  if (contentType) {
    headers.set('content-type', contentType);
  }
  const authHeader = request.headers.get('authorization');
  if (authHeader) {
    headers.set('authorization', authHeader);
  } else if (apiKey) {
    headers.set('authorization', `Bearer ${apiKey}`);
  }

  const body = request.body;

  const response = await fetch(targetUrl, {
    method: request.method,
    headers,
    body: body ? (body as any) : undefined,
    duplex: 'half',
  } as RequestInit);

  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: {
      'content-type': response.headers.get('content-type') || 'application/json',
    },
  });
};
```

- [ ] **Step 2: Write API helper**

`src/lib/api.ts`:
```typescript
export async function apiFetch(path: string, options?: RequestInit) {
  return fetch(`/api/v1${path}`, options);
}

export async function apiFetchJson<T = unknown>(path: string, options?: RequestInit): Promise<T> {
  const res = await apiFetch(path, options);
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error || `API error: ${res.status}`);
  }
  return res.json();
}
```

- [ ] **Step 3: Write shared types**

`src/lib/types.ts`:
```typescript
export interface Product {
  id: string;
  title: string;
  slug: string;
  description: string | null;
  price: string;
  compare_at_price: string | null;
  currency: string;
  inventory_quantity: number;
  is_active: boolean;
  is_featured: boolean;
  images: { id: string; src: string; alt_text: string | null }[];
  variants: { id: string; title: string; sku: string; price: string; inventory_quantity: number }[];
  created_at: string;
  updated_at: string;
}

export interface CartItem {
  id: string;
  product_id: string;
  variant_id: string | null;
  title: string;
  sku: string | null;
  quantity: number;
  unit_price: string;
  total: string;
}

export interface Cart {
  id: string;
  customer_id: string | null;
  subtotal: string;
  discount_total: string;
  tax_total: string;
  total: string;
  currency: string;
  items: CartItem[];
}

export interface Customer {
  id: string;
  email: string;
  first_name: string;
  last_name: string;
}

export interface CheckoutSummary {
  cart_id: string;
  items: CartItem[];
  subtotal: string;
  discount_total: string;
  shipping_total: string;
  tax_total: string;
  total: string;
  currency: string;
  available_shipping_rates: ShippingRate[];
  selected_shipping_rate: ShippingRate | null;
}

export interface ShippingRate {
  provider_id: string;
  carrier: string;
  service_code: string;
  service_name: string;
  rate: string;
  currency: string;
  delivery_days: number | null;
  total_cost: string;
}
```

- [ ] **Step 4: Verify proxy compiles**

Run:
```bash
cd demo-muying && npx astro build
```

Expected: Build completes without TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add demo-muying/src/pages/api demo-muying/src/lib
git commit -m "feat: add API proxy and shared types"
```

---

### Task 3: Layout and Global Styles

**Files:**
- Create: `demo-muying/src/styles/global.css`
- Create: `demo-muying/src/layouts/Layout.astro`

- [ ] **Step 1: Write global.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

html {
  scroll-behavior: smooth;
}

body {
  @apply bg-muying-50 text-muying-900 font-sans antialiased;
}

::selection {
  @apply bg-muying-brown text-white;
}

::-webkit-scrollbar {
  width: 6px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: #D6D3CD;
}

::-webkit-scrollbar-thumb:hover {
  background: #57534e;
}

.no-scrollbar::-webkit-scrollbar {
  display: none;
}

.no-scrollbar {
  -ms-overflow-style: none;
  scrollbar-width: none;
}

.fade-in-up {
  animation: fadeInUp 1s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  opacity: 0;
  transform: translateY(40px);
}

@keyframes fadeInUp {
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
```

- [ ] **Step 2: Write Layout.astro**

```astro
---
export interface Props {
  title: string;
}
const { title } = Astro.props;
---

<!DOCTYPE html>
<html lang="en" class="scroll-smooth">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{title}</title>
    <meta name="description" content="MUYING Coffee | 木影咖啡 — Premium coffee sourced from the world's finest regions." />
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600&family=Space+Mono:ital,wght@0,400;0,700;1,400&family=Ma+Shan+Zheng&display=swap" rel="stylesheet" />
  </head>
  <body>
    <slot />
  </body>
</html>
```

- [ ] **Step 3: Commit**

```bash
git add demo-muying/src/styles demo-muying/src/layouts
git commit -m "feat: add layout and global styles"
```

---

### Task 4: Home Page Sections (Static Brand)

**Files:**
- Create: `demo-muying/src/components/Nav.astro`
- Create: `demo-muying/src/components/Hero.astro`
- Create: `demo-muying/src/components/About.astro`
- Create: `demo-muying/src/components/AppSection.astro`
- Create: `demo-muying/src/components/GuidesSection.astro`
- Create: `demo-muying/src/components/BlogSection.astro`
- Create: `demo-muying/src/components/Footer.astro`
- Create: `demo-muying/src/pages/index.astro`

- [ ] **Step 1: Write Nav.astro**

```astro
<nav id="main-nav" class="absolute top-0 w-full z-50 p-8 md:p-12 mix-blend-difference text-white pointer-events-none transition-opacity duration-300">
  <div class="flex justify-between items-start pointer-events-auto">
    <a href="/" class="flex flex-col gap-1 group">
      <div class="flex items-center gap-2">
        <span class="font-chinese text-xl leading-none">木影</span>
        <span class="font-mono text-sm uppercase tracking-widest leading-none">MUYING</span>
      </div>
      <span class="text-[10px] font-mono tracking-widest opacity-60 group-hover:opacity-100 transition-opacity">EST. GLOBAL</span>
    </a>

    <div id="desktop-menu" class="hidden md:flex flex-col items-end gap-2 font-mono text-xs uppercase tracking-widest">
      <a href="#about" class="hover:line-through decoration-1 transition-all">01. Origin</a>
      <a href="#app" class="hover:line-through decoration-1 transition-all">02. App</a>
      <a href="#shop" class="hover:line-through decoration-1 transition-all">03. Shop</a>
      <a href="#guides" class="hover:line-through decoration-1 transition-all">04. Brew</a>
      <a href="#blog" class="hover:line-through decoration-1 transition-all">05. Journal</a>
      <button id="nav-cart-btn" class="mt-4 flex items-center gap-2 hover:opacity-70 group">
        <span id="nav-cart-count">Cart (0)</span>
      </button>
    </div>

    <button id="mobile-menu-btn" class="md:hidden pointer-events-auto">
      <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-6 h-6"><line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="18" y2="18"/></svg>
    </button>
  </div>
</nav>
```

- [ ] **Step 2: Write Hero.astro**

```astro
<header class="relative min-h-screen flex items-center justify-center overflow-hidden bg-muying-brown">
  <div class="absolute inset-0 z-0">
    <img src="https://images.unsplash.com/photo-1511497584788-876760111969?q=80&w=2670&auto=format&fit=crop" alt="Misty Forest" class="w-full h-full object-cover mix-blend-overlay opacity-30" />
  </div>
  <div class="absolute inset-0 z-0 bg-gradient-to-b from-transparent to-muying-brown/50"></div>

  <div class="relative z-10 w-full px-8 md:px-12 pt-20">
    <div class="max-w-7xl mx-auto border-t border-white/20 pt-8">
      <div class="grid grid-cols-1 md:grid-cols-12 gap-8">
        <div class="md:col-span-8">
          <h1 class="text-white">
            <span class="block font-chinese text-6xl md:text-8xl mb-2">木影</span>
            <span class="block font-mono text-5xl md:text-7xl lg:text-8xl uppercase leading-[0.9] tracking-tighter">
              The Spirit<br/>of Wood
            </span>
          </h1>
        </div>
        <div class="md:col-span-4 flex flex-col justify-between h-full">
          <p class="font-sans font-light text-sm md:text-base text-muying-300 leading-relaxed max-w-md ml-auto md:ml-0 md:mt-4">
            In Chinese philosophy, the Wood element represents upward growth, vitality, and the nurturing energy of spring. MUYING (木影) embodies this ethos.
          </p>
          <div class="mt-12 md:mt-0">
            <a href="#shop" class="inline-flex items-center gap-4 group cursor-pointer">
              <div class="w-12 h-12 border border-white rounded-full flex items-center justify-center group-hover:bg-white group-hover:text-muying-brown transition-all duration-500 text-white">
                <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4"><path d="M12 5v14"/><path d="m19 12-7 7-7-7"/></svg>
              </div>
              <span class="font-mono text-xs uppercase tracking-widest text-white">Explore Collection</span>
            </a>
          </div>
        </div>
      </div>
    </div>
  </div>
</header>
```

- [ ] **Step 3: Write About.astro**

```astro
<section id="about" class="min-h-screen flex items-center py-24 md:py-0 bg-white border-t border-muying-900/5">
  <div class="container mx-auto px-8 md:px-12">
    <div class="grid md:grid-cols-2 gap-24 items-center">
      <div class="order-2 md:order-1 space-y-12">
        <div class="space-y-2">
          <span class="font-mono text-xs uppercase tracking-widest text-muying-500">01. The Philosophy</span>
          <h2 class="font-mono text-4xl md:text-6xl uppercase leading-none tracking-tight text-muying-900">
            Benevolence<br/>and Growth
          </h2>
        </div>
        <div class="space-y-8 font-sans font-light text-muying-700 leading-relaxed max-w-md">
          <p>The Wood element (Mu) is associated with benevolence (Ren) and the persistence of life. It teaches us to be flexible yet strong.</p>
          <p>Our brand, "Wood Shadow" (Muying), honours this relationship. We select beans from ecosystems where the coffee tree thrives beneath the canopy.</p>
        </div>
        <div class="grid grid-cols-2 gap-8 border-t border-muying-200 pt-8 font-mono text-xs uppercase">
          <div>
            <span class="block text-2xl mb-1 text-muying-900">Global</span>
            <span class="text-muying-500">Origin</span>
          </div>
          <div>
            <span class="block text-2xl mb-1 text-muying-900">Wu Xing</span>
            <span class="text-muying-500">Philosophy</span>
          </div>
        </div>
      </div>
      <div class="order-1 md:order-2 relative h-[60vh] md:h-[80vh] w-full">
        <div class="absolute inset-0 bg-muying-200 overflow-hidden">
          <img src="https://images.unsplash.com/photo-1524350876685-274059332603?q=80&w=2670&auto=format&fit=crop" alt="Coffee Forest Shade" class="w-full h-full object-cover hover:scale-105 transition-transform duration-[1.5s] grayscale hover:grayscale-0" />
        </div>
        <div class="absolute -left-12 top-1/2 -rotate-90 origin-center hidden md:block">
          <span class="font-mono text-xs uppercase tracking-[0.3em] text-muying-400">Shade Grown Forests</span>
        </div>
      </div>
    </div>
  </div>
</section>
```

- [ ] **Step 4: Write AppSection.astro**

```astro
<section id="app" class="py-32 bg-muying-50 border-t border-muying-200 overflow-hidden">
  <div class="container mx-auto px-8 md:px-12">
    <div class="flex flex-col lg:flex-row items-center gap-16">
      <div class="lg:w-1/2 space-y-8 relative z-20">
        <span class="font-mono text-xs uppercase tracking-widest text-muying-500">Mobile Experience</span>
        <h2 class="font-mono text-4xl md:text-6xl uppercase leading-none tracking-tight text-muying-900">
          The Ritual<br/>in Your Pocket
        </h2>
        <p class="font-sans font-light text-muying-700 leading-relaxed max-w-md">
          Shop your favourite roasts, manage your subscription, and master the art of brewing with our built-in precision timers.
        </p>
        <div class="flex flex-col sm:flex-row gap-4 pt-4">
          <button class="bg-black text-white px-6 py-3 rounded-lg flex items-center gap-3 hover:opacity-80 transition-opacity">
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="currentColor" class="w-6 h-6"><path d="M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.372-2-.156-3.675 1.09-4.61 1.09zM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.559-1.701"/></svg>
            <div class="text-left">
              <span class="block text-[9px] uppercase leading-none opacity-80">Download on the</span>
              <span class="block text-sm font-bold leading-none mt-1">App Store</span>
            </div>
          </button>
          <button class="bg-black text-white px-6 py-3 rounded-lg flex items-center gap-3 hover:opacity-80 transition-opacity">
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="currentColor" class="w-6 h-6"><path d="M3.609 1.814L13.792 12 3.61 22.186a.996.996 0 0 1-.61-.92V2.734a1 1 0 0 1 .609-.92zm10.89 10.893l2.302 2.302-10.937 6.333 8.635-8.635zm3.199-3.198l2.807 1.626a1 1 0 0 1 0 1.73l-2.808 1.626L15.206 12l2.492-2.491zM5.864 2.658L16.8 8.99l-2.302 2.302-8.634-8.634z"/></svg>
            <div class="text-left">
              <span class="block text-[9px] uppercase leading-none opacity-80">Get it on</span>
              <span class="block text-sm font-bold leading-none mt-1">Google Play</span>
            </div>
          </button>
        </div>
      </div>
      <div class="lg:w-1/2 relative flex justify-center items-center h-[400px]">
        <div class="absolute flex gap-8 transform scale-75 md:scale-90 origin-center">
          <div class="relative w-[280px] h-[560px] bg-muying-50 rounded-[32px] shadow-2xl overflow-hidden border-[6px] border-muying-900 transform -rotate-3 translate-y-4 z-10">
            <img src="https://images.unsplash.com/photo-1511497584788-876760111969?q=80&w=600&auto=format&fit=crop" class="w-full h-full object-cover" alt="App mockup" />
          </div>
          <div class="relative w-[280px] h-[560px] bg-muying-50 rounded-[32px] shadow-2xl overflow-hidden border-[6px] border-muying-900 transform rotate-3 z-20">
            <img src="https://images.unsplash.com/photo-1495474472287-4d71bcdd2085?q=80&w=600&auto=format&fit=crop" class="w-full h-full object-cover" alt="App mockup" />
          </div>
        </div>
      </div>
    </div>
  </div>
</section>
```

- [ ] **Step 5: Write GuidesSection.astro**

```astro
<section id="guides" class="min-h-screen py-32 bg-muying-50">
  <div class="container mx-auto px-8 md:px-12">
    <div class="space-y-2 mb-16">
      <span class="font-mono text-xs uppercase tracking-widest text-muying-500">04. Brew</span>
      <h2 class="font-mono text-4xl md:text-6xl uppercase leading-none tracking-tight text-muying-900">Brew Guides</h2>
    </div>
    <div class="grid md:grid-cols-3 gap-8">
      <a href="/guides/v60" class="group block bg-white p-8 border border-muying-200 hover:border-muying-900 transition-colors">
        <h3 class="font-mono text-xl uppercase mb-2">V60 Pour Over</h3>
        <p class="font-sans font-light text-muying-600 text-sm">Clean, bright, and nuanced. The classic Japanese method.</p>
      </a>
      <a href="/guides/espresso" class="group block bg-white p-8 border border-muying-200 hover:border-muying-900 transition-colors">
        <h3 class="font-mono text-xl uppercase mb-2">Espresso</h3>
        <p class="font-sans font-light text-muying-600 text-sm">Intense, concentrated, and full-bodied. The heart of Italian coffee.</p>
      </a>
      <a href="/guides/french-press" class="group block bg-white p-8 border border-muying-200 hover:border-muying-900 transition-colors">
        <h3 class="font-mono text-xl uppercase mb-2">French Press</h3>
        <p class="font-sans font-light text-muying-600 text-sm">Rich, bold, and forgiving. Perfect for slow mornings.</p>
      </a>
    </div>
  </div>
</section>
```

- [ ] **Step 6: Write BlogSection.astro**

```astro
<section id="blog" class="min-h-screen py-32 bg-white border-t border-muying-100">
  <div class="container mx-auto px-8 md:px-12">
    <div class="space-y-2 mb-16">
      <span class="font-mono text-xs uppercase tracking-widest text-muying-500">05. Journal</span>
      <h2 class="font-mono text-4xl md:text-6xl uppercase leading-none tracking-tight text-muying-900">Journal</h2>
    </div>
    <div class="grid md:grid-cols-2 gap-8">
      <a href="/blog/yunnan-origin" class="group block">
        <div class="aspect-[16/9] bg-muying-200 overflow-hidden mb-4">
          <img src="https://images.unsplash.com/photo-1447933601403-0c6688de566e?q=80&w=800&auto=format&fit=crop" alt="Yunnan" class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-700" />
        </div>
        <span class="font-mono text-[10px] uppercase tracking-widest text-muying-500">Origin Story</span>
        <h3 class="font-mono text-xl uppercase mt-1 group-hover:text-muying-500 transition-colors">The Misty Mountains of Yunnan</h3>
      </a>
      <a href="/blog/roasting-wood" class="group block">
        <div class="aspect-[16/9] bg-muying-200 overflow-hidden mb-4">
          <img src="https://images.unsplash.com/photo-1514432324607-a09d9b4aefdd?q=80&w=800&auto=format&fit=crop" alt="Roasting" class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-700" />
        </div>
        <span class="font-mono text-[10px] uppercase tracking-widest text-muying-500">Process</span>
        <h3 class="font-mono text-xl uppercase mt-1 group-hover:text-muying-500 transition-colors">Roasting with the Wood Element</h3>
      </a>
    </div>
  </div>
</section>
```

- [ ] **Step 7: Write Footer.astro**

```astro
<footer class="bg-muying-900 text-muying-300 py-24 border-t border-white/5">
  <div class="container mx-auto px-8 md:px-12">
    <div class="grid md:grid-cols-4 gap-12">
      <div class="space-y-4">
        <div class="flex items-center gap-2">
          <span class="font-chinese text-xl text-white">木影</span>
          <span class="font-mono text-sm uppercase tracking-widest text-white">MUYING</span>
        </div>
        <p class="font-sans font-light text-sm text-muying-500">Embodying the vitality of the Wood element. Premium coffee sourced from the world's finest regions.</p>
      </div>
      <div>
        <h4 class="font-mono text-xs uppercase tracking-widest text-white mb-4">Shop</h4>
        <ul class="space-y-2 font-sans font-light text-sm">
          <li><a href="#shop" class="hover:text-white transition-colors">All Coffee</a></li>
          <li><a href="#shop" class="hover:text-white transition-colors">Subscriptions</a></li>
          <li><a href="#shop" class="hover:text-white transition-colors">Equipment</a></li>
        </ul>
      </div>
      <div>
        <h4 class="font-mono text-xs uppercase tracking-widest text-white mb-4">Company</h4>
        <ul class="space-y-2 font-sans font-light text-sm">
          <li><a href="#about" class="hover:text-white transition-colors">About</a></li>
          <li><a href="#blog" class="hover:text-white transition-colors">Journal</a></li>
          <li><a href="#contact" class="hover:text-white transition-colors">Contact</a></li>
        </ul>
      </div>
      <div>
        <h4 class="font-mono text-xs uppercase tracking-widest text-white mb-4">Contact</h4>
        <p class="font-sans font-light text-sm">hello@muyingcoffee.com</p>
      </div>
    </div>
    <div class="mt-16 pt-8 border-t border-white/10 font-mono text-[10px] uppercase tracking-widest text-muying-500 flex flex-col md:flex-row justify-between gap-4">
      <span>&copy; {new Date().getFullYear()} MUYING Coffee. All rights reserved.</span>
      <div class="flex gap-6">
        <a href="/privacy" class="hover:text-white transition-colors">Privacy</a>
        <a href="/terms" class="hover:text-white transition-colors">Terms</a>
      </div>
    </div>
  </div>
</footer>
```

- [ ] **Step 8: Write index.astro**

```astro
---
import Layout from '../layouts/Layout.astro';
import Nav from '../components/Nav.astro';
import Hero from '../components/Hero.astro';
import About from '../components/About.astro';
import AppSection from '../components/AppSection.astro';
import ShopSection from '../components/ShopSection.astro';
import GuidesSection from '../components/GuidesSection.astro';
import BlogSection from '../components/BlogSection.astro';
import Footer from '../components/Footer.astro';
---

<Layout title="MUYING Coffee | 木影咖啡">
  <Nav />
  <main>
    <Hero />
    <About />
    <AppSection />
    <ShopSection />
    <GuidesSection />
    <BlogSection />
  </main>
  <Footer />
</Layout>
```

- [ ] **Step 9: Verify build**

Run:
```bash
cd demo-muying && npx astro build
```

Expected: Build completes (ShopSection will be missing until Task 5, so this step is just verifying static sections compile).

- [ ] **Step 10: Commit**

```bash
git add demo-muying/src/components demo-muying/src/pages/index.astro
git commit -m "feat: add home page static brand sections"
```

---

### Task 5: Shop Section and Product Pages

**Files:**
- Create: `demo-muying/src/components/ShopSection.astro`
- Create: `demo-muying/src/components/ProductCard.astro`
- Create: `demo-muying/src/pages/products/[id].astro`

- [ ] **Step 1: Write ProductCard.astro**

```astro
---
import type { Product } from '../lib/types';

interface Props {
  product: Product;
}

const { product } = Astro.props;
const image = product.images[0]?.src || 'https://images.unsplash.com/photo-1559056199-641a0ac8b55e?q=80&w=600&auto=format&fit=crop';
---

<div class="group cursor-pointer" data-product-id={product.id}>
  <div class="aspect-[4/5] bg-muying-200 overflow-hidden mb-4">
    <img src={image} alt={product.images[0]?.alt_text || product.title} class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-700" />
  </div>
  <h3 class="font-mono text-sm uppercase tracking-widest text-muying-900">{product.title}</h3>
  <p class="font-sans font-light text-muying-600 mt-1">${product.price} / {product.currency}</p>
</div>
```

- [ ] **Step 2: Write ShopSection.astro**

```astro
---
import { apiFetchJson } from '../lib/api';
import type { Product } from '../lib/types';
import ProductCard from './ProductCard.astro';

let products: Product[] = [];
try {
  const data = await apiFetchJson<{ products: Product[]; meta: any }>('/products');
  products = data.products.filter((p) => p.is_active);
} catch (e) {
  console.error('Failed to fetch products', e);
}
---

<section id="shop" class="min-h-screen py-32 bg-white">
  <div class="container mx-auto px-8 md:px-12">
    <div class="space-y-2 mb-16">
      <span class="font-mono text-xs uppercase tracking-widest text-muying-500">03. Shop</span>
      <h2 class="font-mono text-4xl md:text-6xl uppercase leading-none tracking-tight text-muying-900">Collection</h2>
    </div>
    {products.length > 0 ? (
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-8">
        {products.map((product) => (
          <a href={`/products/${product.id}`}>
            <ProductCard product={product} />
          </a>
        ))}
      </div>
    ) : (
      <div class="text-center py-24">
        <p class="font-sans font-light text-muying-600">No products available right now.</p>
      </div>
    )}
  </div>
</section>
```

- [ ] **Step 3: Write product detail page**

`src/pages/products/[id].astro`:
```astro
---
import Layout from '../../layouts/Layout.astro';
import Nav from '../../components/Nav.astro';
import Footer from '../../components/Footer.astro';
import { apiFetchJson } from '../../lib/api';
import type { Product } from '../../lib/types';

const { id } = Astro.params;
let product: Product | null = null;

try {
  const data = await apiFetchJson<{ product: Product }>(`/products/${id}`);
  product = data.product;
} catch (e) {
  console.error('Failed to fetch product', e);
}

if (!product) {
  return new Response('Product not found', { status: 404 });
}

const image = product.images[0]?.src || 'https://images.unsplash.com/photo-1559056199-641a0ac8b55e?q=80&w=800&auto=format&fit=crop';
---

<Layout title={`${product.title} | MUYING Coffee`}>
  <Nav />
  <main class="pt-32 pb-24 bg-white min-h-screen">
    <div class="container mx-auto px-8 md:px-12">
      <div class="grid md:grid-cols-2 gap-16 items-start">
        <div class="aspect-square bg-muying-200 overflow-hidden">
          <img src={image} alt={product.images[0]?.alt_text || product.title} class="w-full h-full object-cover" />
        </div>
        <div class="space-y-8">
          <div>
            <span class="font-mono text-xs uppercase tracking-widest text-muying-500">{product.currency}</span>
            <h1 class="font-mono text-4xl md:text-5xl uppercase text-muying-900 leading-tight mt-2">{product.title}</h1>
            <p class="font-sans font-light text-2xl text-muying-700 mt-4">${product.price}</p>
          </div>
          <p class="font-sans font-light text-muying-600 leading-relaxed">{product.description || 'A carefully sourced coffee that embodies the spirit of wood.'}</p>
          {product.variants.length > 0 && (
            <div>
              <label class="font-mono text-xs uppercase tracking-widest text-muying-500 block mb-2">Variant</label>
              <select id="variant-select" class="w-full border border-muying-300 px-4 py-3 font-sans text-sm focus:outline-none focus:border-muying-900">
                {product.variants.map((v) => (
                  <option value={v.id}>{v.title} — ${v.price}</option>
                ))}
              </select>
            </div>
          )}
          <div class="flex gap-4">
            <div class="w-24">
              <label class="font-mono text-xs uppercase tracking-widest text-muying-500 block mb-2">Qty</label>
              <input type="number" id="product-qty" value="1" min="1" class="w-full border border-muying-300 px-4 py-3 font-sans text-sm text-center focus:outline-none focus:border-muying-900" />
            </div>
            <button id="add-to-cart-btn" data-product-id={product.id} class="flex-1 bg-muying-900 text-white font-mono text-sm uppercase tracking-widest py-3 hover:bg-muying-700 transition-colors">
              Add to Cart
            </button>
          </div>
        </div>
      </div>
    </div>
  </main>
  <Footer />
</Layout>
```

- [ ] **Step 4: Verify build**

Run:
```bash
cd demo-muying && npx astro build
```

Expected: Build completes without errors.

- [ ] **Step 5: Commit**

```bash
git add demo-muying/src/components/ShopSection.astro demo-muying/src/components/ProductCard.astro demo-muying/src/pages/products
git commit -m "feat: add shop section and product detail pages"
```

---

### Task 6: Cart Drawer (Client Island)

**Files:**
- Create: `demo-muying/src/components/CartDrawer.tsx`
- Create: `demo-muying/src/lib/cart.ts`
- Modify: `demo-muying/src/layouts/Layout.astro`

- [ ] **Step 1: Write cart.ts**

```typescript
import type { Cart } from './types';

const CART_KEY = 'muying_cart';

export function getStoredCartId(): string | null {
  return localStorage.getItem(CART_KEY);
}

export function setStoredCartId(cartId: string) {
  localStorage.setItem(CART_KEY, cartId);
}

export async function getOrCreateCart(): Promise<Cart> {
  const existingId = getStoredCartId();
  if (existingId) {
    try {
      const res = await fetch(`/api/v1/carts/${existingId}`);
      if (res.ok) return res.json();
    } catch {
      // ignore, create new cart
    }
  }
  const res = await fetch('/api/v1/carts/guest', { method: 'POST' });
  const cart: Cart = await res.json();
  setStoredCartId(cart.id);
  return cart;
}

export async function addToCart(cartId: string, productId: string, quantity: number, variantId?: string) {
  return fetch(`/api/v1/carts/${cartId}/items`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ product_id: productId, variant_id: variantId || null, quantity }),
  });
}

export async function updateCartItem(cartId: string, itemId: string, quantity: number) {
  return fetch(`/api/v1/carts/${cartId}/items/${itemId}`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ quantity }),
  });
}

export async function removeCartItem(cartId: string, itemId: string) {
  return fetch(`/api/v1/carts/${cartId}/items/${itemId}`, { method: 'DELETE' });
}
```

- [ ] **Step 2: Write CartDrawer.tsx**

```tsx
import { useState, useEffect } from 'react';
import type { Cart, CartItem } from '../lib/types';
import { getOrCreateCart, updateCartItem, removeCartItem } from '../lib/cart';

export default function CartDrawer() {
  const [isOpen, setIsOpen] = useState(false);
  const [cart, setCart] = useState<Cart | null>(null);

  const refreshCart = async () => {
    const c = await getOrCreateCart();
    setCart(c);
  };

  useEffect(() => {
    refreshCart();
    const onOpen = () => setIsOpen(true);
    window.addEventListener('open-cart', onOpen);
    window.addEventListener('cart-updated', refreshCart);
    return () => {
      window.removeEventListener('open-cart', onOpen);
      window.removeEventListener('cart-updated', refreshCart);
    };
  }, []);

  const handleUpdate = async (itemId: string, qty: number) => {
    if (!cart) return;
    if (qty <= 0) {
      await removeCartItem(cart.id, itemId);
    } else {
      await updateCartItem(cart.id, itemId, qty);
    }
    await refreshCart();
  };

  const totalItems = cart?.items.reduce((sum, item) => sum + item.quantity, 0) || 0;

  return (
    <>
      <button onClick={() => setIsOpen(true)} className="fixed bottom-6 right-6 z-40 w-14 h-14 bg-muying-900 text-white rounded-full shadow-lg flex items-center justify-center hover:bg-muying-700 transition-colors">
        <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="w-5 h-5"><circle cx="9" cy="21" r="1"/><circle cx="20" cy="21" r="1"/><path d="M1 1h4l2.68 13.39a2 2 0 0 0 2 1.61h9.72a2 2 0 0 0 2-1.61L23 6H6"/></svg>
        {totalItems > 0 && (
          <span className="absolute -top-1 -right-1 w-5 h-5 bg-muying-brown text-white text-[10px] font-bold rounded-full flex items-center justify-center">{totalItems}</span>
        )}
      </button>

      {isOpen && (
        <div className="fixed inset-0 z-50">
          <div onClick={() => setIsOpen(false)} className="absolute inset-0 bg-black/50 backdrop-blur-sm" />
          <div className="absolute top-0 right-0 h-full w-full md:w-[450px] bg-white shadow-2xl flex flex-col">
            <div className="p-6 border-b border-muying-100 flex justify-between items-center">
              <h2 className="font-mono text-sm uppercase tracking-widest">Your Cart</h2>
              <button onClick={() => setIsOpen(false)} className="text-muying-500 hover:text-muying-900">
                <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="w-5 h-5"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
              </button>
            </div>

            <div className="flex-1 overflow-y-auto p-6">
              {!cart || cart.items.length === 0 ? (
                <div className="h-full flex flex-col items-center justify-center text-center">
                  <p className="font-sans font-light text-muying-600">Your cart is empty.</p>
                </div>
              ) : (
                <div className="space-y-6">
                  {cart.items.map((item) => (
                    <div key={item.id} className="flex gap-4">
                      <div className="flex-1">
                        <h3 className="font-mono text-sm uppercase text-muying-900">{item.title}</h3>
                        <p className="font-sans font-light text-xs text-muying-500">${item.unit_price} / {cart.currency}</p>
                      </div>
                      <div className="flex items-center gap-2">
                        <button onClick={() => handleUpdate(item.id, item.quantity - 1)} className="w-8 h-8 border border-muying-200 flex items-center justify-center hover:border-muying-900">-</button>
                        <span className="w-8 text-center font-mono text-sm">{item.quantity}</span>
                        <button onClick={() => handleUpdate(item.id, item.quantity + 1)} className="w-8 h-8 border border-muying-200 flex items-center justify-center hover:border-muying-900">+</button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {cart && cart.items.length > 0 && (
              <div className="p-6 border-t border-muying-100 bg-muying-50">
                <div className="flex justify-between items-center mb-4">
                  <span className="font-mono text-xs uppercase tracking-widest text-muying-500">Subtotal</span>
                  <span className="font-mono text-lg text-muying-900">${cart.total} {cart.currency}</span>
                </div>
                <a href="/checkout" className="block w-full bg-muying-900 text-white text-center font-mono text-sm uppercase tracking-widest py-3 hover:bg-muying-700 transition-colors">
                  Checkout
                </a>
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}
```

- [ ] **Step 3: Add JSX support to tsconfig**

Modify `demo-muying/tsconfig.json`:
```json
{
  "extends": "astro/tsconfigs/strict",
  "compilerOptions": {
    "jsx": "react-jsx",
    "jsxImportSource": "react"
  }
}
```

- [ ] **Step 4: Install React deps**

Run:
```bash
cd demo-muying && npm install react react-dom @types/react @types/react-dom
```

- [ ] **Step 5: Update Layout.astro to include CartDrawer**

```astro
---
export interface Props {
  title: string;
}
const { title } = Astro.props;
import CartDrawer from '../components/CartDrawer.tsx';
---

<!DOCTYPE html>
<html lang="en" class="scroll-smooth">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{title}</title>
    <meta name="description" content="MUYING Coffee | 木影咖啡 — Premium coffee sourced from the world's finest regions." />
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600&family=Space+Mono:ital,wght@0,400;0,700;1,400&family=Ma+Shan+Zheng&display=swap" rel="stylesheet" />
  </head>
  <body>
    <slot />
    <CartDrawer client:load />
  </body>
</html>
```

- [ ] **Step 6: Add add-to-cart script on product page**

At the bottom of `demo-muying/src/pages/products/[id].astro`, before `</Layout>`, add:

```astro
<script>
  import { getOrCreateCart, addToCart } from '../../lib/cart';

  const btn = document.getElementById('add-to-cart-btn');
  if (btn) {
    btn.addEventListener('click', async () => {
      const productId = btn.dataset.productId;
      const qtyInput = document.getElementById('product-qty') as HTMLInputElement;
      const variantSelect = document.getElementById('variant-select') as HTMLSelectElement | null;
      const quantity = parseInt(qtyInput?.value || '1', 10);
      const variantId = variantSelect?.value || undefined;

      const cart = await getOrCreateCart();
      await addToCart(cart.id, productId!, quantity, variantId);
      window.dispatchEvent(new CustomEvent('cart-updated'));
      window.dispatchEvent(new CustomEvent('open-cart'));
    });
  }
</script>
```

- [ ] **Step 7: Verify build**

Run:
```bash
cd demo-muying && npx astro build
```

Expected: Build completes without JSX/React errors.

- [ ] **Step 8: Commit**

```bash
git add demo-muying/src/components/CartDrawer.tsx demo-muying/src/lib/cart.ts demo-muying/src/layouts/Layout.astro demo-muying/src/pages/products/[id].astro demo-muying/package.json demo-muying/package-lock.json demo-muying/tsconfig.json
git commit -m "feat: add cart drawer and add-to-cart flow"
```

---

### Task 7: Checkout Page

**Files:**
- Create: `demo-muying/src/pages/checkout.astro`
- Create: `demo-muying/src/components/CheckoutForm.tsx`

- [ ] **Step 1: Write CheckoutForm.tsx**

```tsx
import { useState, useEffect } from 'react';
import type { CheckoutSummary, ShippingRate } from '../lib/types';
import { getOrCreateCart } from '../lib/cart';
import { apiFetchJson } from '../lib/api';

export default function CheckoutForm() {
  const [cart, setCart] = useState<any>(null);
  const [summary, setSummary] = useState<CheckoutSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [step, setStep] = useState<'shipping' | 'payment' | 'success'>('shipping');

  const [email, setEmail] = useState('');
  const [address, setAddress] = useState({
    first_name: '',
    last_name: '',
    address_1: '',
    city: '',
    country_code: 'US',
    postal_code: '',
    region_code: '',
    phone: '',
  });

  useEffect(() => {
    getOrCreateCart().then(setCart);
  }, []);

  const initiateCheckout = async () => {
    if (!cart) return;
    setLoading(true);
    setError(null);
    try {
      const payload = {
        cart_id: cart.id,
        shipping_address: {
          ...address,
          address_2: null,
          company: null,
        },
        billing_address: null,
        currency: cart.currency,
      };
      const res = await apiFetchJson<CheckoutSummary>('/checkout/initiate', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload),
      });
      setSummary(res);
      setStep('payment');
    } catch (e: any) {
      setError(e.message || 'Failed to initiate checkout');
    } finally {
      setLoading(false);
    }
  };

  const selectShipping = async (rate: ShippingRate) => {
    if (!cart || !summary) return;
    setLoading(true);
    try {
      const res = await apiFetchJson<CheckoutSummary>('/checkout/shipping', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          cart_id: cart.id,
          shipping_rate: rate,
        }),
      });
      setSummary(res);
    } catch (e: any) {
      setError(e.message || 'Failed to select shipping');
    } finally {
      setLoading(false);
    }
  };

  const completeCheckout = async () => {
    if (!cart || !summary) return;
    setLoading(true);
    setError(null);
    try {
      const payload = {
        cart_id: cart.id,
        shipping_address: {
          ...address,
          address_2: null,
          company: null,
        },
        billing_address: null,
        payment_method: {
          type: 'card',
          number: '4242424242424242',
          exp_month: 12,
          exp_year: 2026,
          cvc: '123',
          name: `${address.first_name} ${address.last_name}`,
        },
        customer_email: email,
        selected_shipping_rate: summary.selected_shipping_rate || summary.available_shipping_rates[0],
      };
      await apiFetchJson('/checkout/complete', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload),
      });
      localStorage.removeItem('muying_cart');
      setStep('success');
    } catch (e: any) {
      setError(e.message || 'Payment failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="max-w-xl mx-auto">
      {step === 'shipping' && (
        <div className="space-y-6">
          <h2 className="font-mono text-xl uppercase tracking-tight">Shipping Details</h2>
          <input type="email" placeholder="Email" value={email} onChange={(e) => setEmail(e.target.value)} className="w-full border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
          <div className="grid grid-cols-2 gap-4">
            <input placeholder="First name" value={address.first_name} onChange={(e) => setAddress({ ...address, first_name: e.target.value })} className="border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
            <input placeholder="Last name" value={address.last_name} onChange={(e) => setAddress({ ...address, last_name: e.target.value })} className="border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
          </div>
          <input placeholder="Address" value={address.address_1} onChange={(e) => setAddress({ ...address, address_1: e.target.value })} className="w-full border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
          <div className="grid grid-cols-2 gap-4">
            <input placeholder="City" value={address.city} onChange={(e) => setAddress({ ...address, city: e.target.value })} className="border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
            <input placeholder="Postal code" value={address.postal_code} onChange={(e) => setAddress({ ...address, postal_code: e.target.value })} className="border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <input placeholder="Country code (e.g. US)" value={address.country_code} onChange={(e) => setAddress({ ...address, country_code: e.target.value })} className="border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
            <input placeholder="State/Region code" value={address.region_code} onChange={(e) => setAddress({ ...address, region_code: e.target.value })} className="border border-muying-300 px-4 py-3 text-sm focus:outline-none focus:border-muying-900" />
          </div>
          {error && <p className="text-red-600 text-sm">{error}</p>}
          <button onClick={initiateCheckout} disabled={loading} className="w-full bg-muying-900 text-white font-mono text-sm uppercase tracking-widest py-3 hover:bg-muying-700 transition-colors disabled:opacity-50">
            {loading ? 'Calculating...' : 'Continue to Shipping'}
          </button>
        </div>
      )}

      {step === 'payment' && summary && (
        <div className="space-y-6">
          <h2 className="font-mono text-xl uppercase tracking-tight">Shipping Method</h2>
          <div className="space-y-3">
            {summary.available_shipping_rates.map((rate) => (
              <label key={rate.service_code} className={`block p-4 border cursor-pointer ${summary.selected_shipping_rate?.service_code === rate.service_code ? 'border-muying-900 bg-muying-50' : 'border-muying-200'}`}>
                <input
                  type="radio"
                  name="shipping"
                  className="mr-3"
                  checked={summary.selected_shipping_rate?.service_code === rate.service_code}
                  onChange={() => selectShipping(rate)}
                />
                <span className="font-mono text-sm uppercase">{rate.carrier} — {rate.service_name}</span>
                <span className="float-right font-sans">${rate.total_cost}</span>
              </label>
            ))}
          </div>

          <div className="border-t border-muying-200 pt-6 space-y-2">
            <div className="flex justify-between font-sans text-sm"><span>Subtotal</span><span>${summary.subtotal}</span></div>
            <div className="flex justify-between font-sans text-sm"><span>Shipping</span><span>${summary.shipping_total}</span></div>
            <div className="flex justify-between font-sans text-sm"><span>Tax</span><span>${summary.tax_total}</span></div>
            <div className="flex justify-between font-mono text-lg pt-2"><span>Total</span><span>${summary.total} {summary.currency}</span></div>
          </div>

          {error && <p className="text-red-600 text-sm">{error}</p>}
          <button onClick={completeCheckout} disabled={loading} className="w-full bg-muying-900 text-white font-mono text-sm uppercase tracking-widest py-3 hover:bg-muying-700 transition-colors disabled:opacity-50">
            {loading ? 'Processing...' : 'Complete Order'}
          </button>
        </div>
      )}

      {step === 'success' && (
        <div className="text-center py-12">
          <h2 className="font-mono text-2xl uppercase tracking-tight mb-4">Thank You</h2>
          <p className="font-sans font-light text-muying-600">Your order has been placed successfully.</p>
          <a href="/" className="inline-block mt-8 bg-muying-900 text-white font-mono text-sm uppercase tracking-widest px-8 py-3 hover:bg-muying-700 transition-colors">Continue Shopping</a>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Write checkout.astro**

```astro
---
import Layout from '../layouts/Layout.astro';
import Nav from '../components/Nav.astro';
import Footer from '../components/Footer.astro';
import CheckoutForm from '../components/CheckoutForm.tsx';
---

<Layout title="Checkout | MUYING Coffee">
  <Nav />
  <main class="pt-32 pb-24 bg-white min-h-screen">
    <div class="container mx-auto px-8 md:px-12">
      <h1 class="font-mono text-3xl md:text-4xl uppercase tracking-tight text-muying-900 mb-12 text-center">Checkout</h1>
      <CheckoutForm client:load />
    </div>
  </main>
  <Footer />
</Layout>
```

- [ ] **Step 3: Verify build**

Run:
```bash
cd demo-muying && npx astro build
```

Expected: Build completes.

- [ ] **Step 4: Commit**

```bash
git add demo-muying/src/pages/checkout.astro demo-muying/src/components/CheckoutForm.tsx
git commit -m "feat: add checkout page and form"
```

---

### Task 8: Guides and Blog Detail Pages

**Files:**
- Create: `demo-muying/src/pages/guides/[slug].astro`
- Create: `demo-muying/src/pages/blog/[slug].astro`

- [ ] **Step 1: Write guides page**

`src/pages/guides/[slug].astro`:
```astro
---
import Layout from '../../layouts/Layout.astro';
import Nav from '../../components/Nav.astro';
import Footer from '../../components/Footer.astro';

const { slug } = Astro.params;

const guides: Record<string, { title: string; content: string }> = {
  v60: {
    title: 'V60 Pour Over',
    content: 'Use 15g coffee to 250g water at 96°C. Bloom for 30s, then pour in slow circles. Total brew time: 2:30.',
  },
  espresso: {
    title: 'Espresso',
    content: 'Dose 18g, yield 36g in 27-32 seconds. Look for a rich, syrupy texture and balanced acidity.',
  },
  'french-press': {
    title: 'French Press',
    content: 'Use a 1:15 ratio. Steep for 4 minutes, then plunge gently. Expect a full-bodied, rich cup.',
  },
};

const guide = slug ? guides[slug] : null;

if (!guide) {
  return new Response('Guide not found', { status: 404 });
}
---

<Layout title={`${guide.title} | MUYING Coffee`}>
  <Nav />
  <main class="pt-32 pb-24 bg-white min-h-screen">
    <div class="container mx-auto px-8 md:px-12 max-w-2xl">
      <span class="font-mono text-xs uppercase tracking-widest text-muying-500">Brew Guide</span>
      <h1 class="font-mono text-4xl md:text-5xl uppercase tracking-tight text-muying-900 mt-2 mb-8">{guide.title}</h1>
      <p class="font-sans font-light text-muying-700 leading-relaxed text-lg">{guide.content}</p>
      <a href="/" class="inline-block mt-12 text-muying-500 hover:text-muying-900 font-mono text-xs uppercase tracking-widest">&larr; Back to Home</a>
    </div>
  </main>
  <Footer />
</Layout>
```

- [ ] **Step 2: Write blog page**

`src/pages/blog/[slug].astro`:
```astro
---
import Layout from '../../layouts/Layout.astro';
import Nav from '../../components/Nav.astro';
import Footer from '../../components/Footer.astro';

const { slug } = Astro.params;

const posts: Record<string, { title: string; subtitle: string; content: string }> = {
  'yunnan-origin': {
    title: 'The Misty Mountains of Yunnan',
    subtitle: 'Where the coffee tree finds shelter beneath the canopy.',
    content: 'Yunnan province sits on the southwestern edge of China, bordering Myanmar, Laos, and Vietnam. Its high altitudes, misty mornings, and volcanic soils create the perfect conditions for arabica coffee. At MUYING, we partner with smallholder farms who have been cultivating coffee for generations.',
  },
  'roasting-wood': {
    title: 'Roasting with the Wood Element',
    subtitle: 'Growth, patience, and the fire that transforms.',
    content: 'In the Wu Xing, Wood fuels Fire. Our roasting philosophy honors this cycle: green coffee enters the drum and, through controlled heat, undergoes a transformation. We roast light to preserve the terroir, allowing the origin story of each bean to shine through.',
  },
};

const post = slug ? posts[slug] : null;

if (!post) {
  return new Response('Post not found', { status: 404 });
}
---

<Layout title={`${post.title} | MUYING Coffee`}>
  <Nav />
  <main class="pt-32 pb-24 bg-white min-h-screen">
    <div class="container mx-auto px-8 md:px-12 max-w-2xl">
      <span class="font-mono text-xs uppercase tracking-widest text-muying-500">Journal</span>
      <h1 class="font-mono text-4xl md:text-5xl uppercase tracking-tight text-muying-900 mt-2 mb-4">{post.title}</h1>
      <p class="font-serif italic text-xl text-muying-500 mb-12">{post.subtitle}</p>
      <div class="font-sans font-light text-muying-700 leading-relaxed text-lg space-y-6">
        <p>{post.content}</p>
      </div>
      <a href="/" class="inline-block mt-12 text-muying-500 hover:text-muying-900 font-mono text-xs uppercase tracking-widest">&larr; Back to Home</a>
    </div>
  </main>
  <Footer />
</Layout>
```

- [ ] **Step 3: Verify build**

Run:
```bash
cd demo-muying && npx astro build
```

Expected: Build completes.

- [ ] **Step 4: Commit**

```bash
git add demo-muying/src/pages/guides demo-muying/src/pages/blog
git commit -m "feat: add guides and blog detail pages"
```

---

### Task 9: Final Build and Verification

- [ ] **Step 1: Full build**

Run:
```bash
cd demo-muying && npx astro build
```

Expected: `dist/` created with `_worker.js` and static assets. No errors.

- [ ] **Step 2: Type check**

Run:
```bash
cd demo-muying && npx astro check
```

Expected: No TypeScript errors.

- [ ] **Step 3: Commit final state**

```bash
git add demo-muying
git commit -m "feat: complete muying coffee astro demo frontend"
```

---

## Self-Review

**Spec coverage check:**
- ✅ Astro SSR with Cloudflare Workers — Task 1
- ✅ Secure API proxy — Task 2
- ✅ MUYING brand visuals — Tasks 3-4
- ✅ Product catalog and detail pages — Task 5
- ✅ Cart drawer with API persistence — Task 6
- ✅ Checkout flow wired to rcommerce — Task 7
- ✅ Guides and blog pages — Task 8
- ✅ Build verification — Task 9

**Placeholder scan:** No TBDs, TODOs, or vague steps found.

**Type consistency:** Types in `types.ts` match API expectations and component props.
