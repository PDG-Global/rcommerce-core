export interface Product {
  id: string;
  title: string;
  slug: string;
  description: string | null;
  seo_description: string | null;
  price: string;
  compare_at_price: string | null;
  currency: string;
  product_type?: string;
  inventory_quantity: number;
  is_active: boolean;
  is_featured: boolean;
  sku: string | null;
  /** Global Trade Item Number, when the catalog carries one (8/12/13/14 digits). */
  gtin?: string | null;
  image?: string | null;
  images: { id: string; src: string; alt_text: string | null }[];
  categories: { id: string; name: string; slug: string }[];
  variants: { id: string; title: string; sku: string; price: string; inventory_quantity: number }[];
  options: { id: string; name: string; position: number; values: { id: string; variant_id: string; value: string }[] }[];
  weight?: string | null;
  weight_unit?: string | null;
  average_rating?: number;
  review_count?: number;
  created_at: string;
  updated_at: string;
}

export interface PackOption {
  product_id: string;
  title: string;
  slug: string;
  price: string;
  currency: string;
  inventory_quantity: number;
  label: string;
}

export interface Review {
  id: string;  author_name: string;
  rating: number;
  title?: string | null;
  content: string;
  created_at: string;
}

export interface ReviewSummary {
  reviews: Review[];
  average_rating: number;
  total_reviews: number;
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
  image_url?: string | null;
  /** Backend flags blob; bundle lines carry is_bundle_parent/is_bundle_component. */
  custom_attributes?: Record<string, unknown> | string | null;
}

export interface Cart {
  id: string;
  customer_id: string | null;
  session_token?: string | null;
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
  phone: string | null;
  accepts_marketing: boolean;
  addresses?: Address[];
}

export interface Address {
  id: string;
  first_name: string;
  last_name: string;
  company: string | null;
  phone: string | null;
  address1: string;
  address2: string | null;
  city: string;
  state: string | null;
  country: string;
  zip: string;
  is_default_shipping: boolean;
  is_default_billing: boolean;
}

export interface CheckoutSummary {
  cart_id: string;
  items: CartItem[];
  subtotal: string;
  discount_total: string;
  shipping_total: string;
  item_tax: string;
  tax_total: string;
  total: string;
  currency: string;
  available_shipping_rates: ShippingRate[];
  selected_shipping_rate: ShippingRate | null;
  tax_breakdown: TaxBreakdownItem[];
  vat_id_valid: boolean | null;
  publishable_key: string | null;
  airwallex_client_id: string | null;
  airwallex_env: string | null;
}

export interface CheckoutResult {
  order: Order;
  payment_id: string;
  total_charged: string;
  currency: string;
  client_secret: string | null;
  payment_intent_id: string | null;
  requires_confirmation: boolean;
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

export interface TaxBreakdownItem {
  tax_zone_name: string;
  tax_rate_name: string;
  rate: string;
  taxable_amount: string;
  tax_amount: string;
}

export interface Order {
  id: string;
  order_number: string;
  customer_id: string | null;
  customer_email: string;
  status: string;
  payment_status: string;
  fulfillment_status: string;
  currency: string;
  subtotal: string;
  tax_total: string;
  shipping_total: string;
  discount_total: string;
  total: string;
  items: OrderItem[];
  created_at: string;
}

export interface OrderItem {
  id: string;
  product_id: string;
  variant_id: string | null;
  title: string;
  variant_title?: string | null;
  sku: string | null;
  quantity: number;
  price: string;
  total: string;
  /** Bundle lines: components are $0 child lines hidden from item displays. */
  is_bundle_component?: boolean;
  is_bundle_parent?: boolean;
}
