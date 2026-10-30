import { useState, useEffect, useRef, type FormEvent, type RefObject } from 'react';
import { init as initAirwallex, createElement, type Payment } from '@airwallex/components-sdk';
import type { Cart, CartItem, CheckoutSummary, CheckoutResult, ShippingRate, Customer } from '../lib/types';
import { getOrCreateCart, clearStoredCartId, applyCoupon, formatMoney } from '../lib/cart';
import { apiFetchJson, apiFetchAuthenticated, apiFetchOptionalAuth, AIRWALLEX_ENV } from '../lib/api';
import CountryStateFields from './CountryStateFields';
import { brand } from '../lib/brand';
import { visibleItems } from '../lib/bundles';

const coInput =
  'w-full rounded-md border border-rcommerce-300 bg-white px-3.5 py-3 text-[15px] text-rcommerce-900 placeholder:text-rcommerce-400 focus:outline-none focus:border-rcommerce-accent focus:ring-1 focus:ring-rcommerce-accent transition-colors';

const coBtn =
  'inline-flex items-center justify-center gap-2 rounded-md bg-rcommerce-900 px-8 py-4 text-[15px] font-semibold text-white transition-colors hover:bg-rcommerce-accent focus:outline-none focus:ring-2 focus:ring-rcommerce-accent focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed';

/**
 * Display label for a shipping rate. The API composes names like
 * "Standard Shipping (Standard Shipping)" or "Free Shipping (SF Express)";
 * show just the meaningful title ("SF Express"), appending "— Free" only
 * when the rate costs nothing and the title doesn't already say so.
 */
function shippingRateLabel(rate: Pick<ShippingRate, 'service_name' | 'total_cost'>): string {
  const inner = rate.service_name.match(/^(?:Free|Standard) Shipping \((.+)\)$/);
  const base = inner ? inner[1] : rate.service_name;
  const free = parseFloat(rate.total_cost) === 0;
  return free && !/^free\b/i.test(base) ? `${base} — Free` : base;
}

interface AddressForm {
  id: string;
  customer_id: string;
  first_name: string;
  last_name: string;
  address1: string;
  city: string;
  country: string;
  zip: string;
  state: string;
  phone: string;
  address2: string | null;
  company: string | null;
  is_default_shipping: boolean;
  is_default_billing: boolean;
  created_at: string;
  updated_at: string;
}

const emptyAddress: AddressForm = {
  id: '00000000-0000-0000-0000-000000000000',
  customer_id: '00000000-0000-0000-0000-000000000000',
  first_name: '',
  last_name: '',
  address1: '',
  city: '',
  country: 'US',
  zip: '',
  state: '',
  phone: '',
  address2: null,
  company: null,
  is_default_shipping: false,
  is_default_billing: false,
  created_at: '2024-01-01T00:00:00Z',
  updated_at: '2024-01-01T00:00:00Z',
};

function ReviewRow({ label, value, onChange }: { label: string; value: string; onChange: () => void }) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3 border-b border-rcommerce-200 last:border-b-0">
      <span className="text-sm text-rcommerce-500 w-16 shrink-0">{label}</span>
      <span className="flex-1 text-sm text-rcommerce-900 truncate">{value}</span>
      <button type="button" onClick={onChange} className="text-sm text-rcommerce-accent hover:underline shrink-0">
        Change
      </button>
    </div>
  );
}

function OrderSummary({
  cart,
  summary,
  onApplyDiscount,
  discountCode,
  setDiscountCode,
  discountLoading,
  discountError,
  showDiscountField = true,
}: {
  cart: Cart;
  summary: CheckoutSummary | null;
  onApplyDiscount: () => void;
  discountCode: string;
  setDiscountCode: (v: string) => void;
  discountLoading: boolean;
  discountError: string | null;
  showDiscountField?: boolean;
}) {
  const subtotal = summary?.subtotal ?? cart.subtotal;
  const shipping = summary?.shipping_total;
  const tax = summary?.tax_total ?? cart.tax_total;
  const total = summary?.total ?? cart.total;
  const discount = summary?.discount_total ?? cart.discount_total;
  const currency = summary?.currency ?? cart.currency;

  return (
    <div className="px-4 sm:px-8 lg:pl-12 lg:pr-8 xl:pr-24 py-8 lg:py-12 max-w-[560px] lg:max-w-none lg:w-[460px] xl:w-[500px] mx-auto lg:mx-0 lg:mr-auto lg:sticky lg:top-0">
      <div className="space-y-4">
        {visibleItems(cart.items).map((item: CartItem) => (
          <div key={item.id} className="flex items-center gap-4">
            <div className="relative w-16 h-16 shrink-0">
              <div className="w-16 h-16 rounded-md border border-rcommerce-200 bg-white overflow-hidden">
                {item.image_url ? (
                  <img src={item.image_url} alt={item.title} className="w-full h-full object-cover" loading="lazy" />
                ) : (
                  <div className="w-full h-full" />
                )}
              </div>
              <span className="absolute -top-2 -right-2 min-w-[20px] h-5 px-1 bg-rcommerce-700 text-white text-[11px] font-semibold rounded-full flex items-center justify-center">
                {item.quantity}
              </span>
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-rcommerce-900 line-clamp-2">{item.title}</p>
            </div>
            <p className="text-sm font-medium text-rcommerce-900 whitespace-nowrap">
              {currency} {formatMoney(item.total)}
            </p>
          </div>
        ))}
      </div>

      {showDiscountField && (
        <div className="mt-6 pt-6 border-t border-rcommerce-200">
          <div className="flex gap-3">
            <input
              type="text"
              value={discountCode}
              onChange={(e) => setDiscountCode(e.target.value)}
              placeholder="Discount code"
              className={coInput + ' flex-1'}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  onApplyDiscount();
                }
              }}
            />
            <button
              type="button"
              onClick={onApplyDiscount}
              disabled={discountLoading || !discountCode.trim()}
              className="btn-secondary px-5 text-[15px] font-semibold disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {discountLoading ? '…' : 'Apply'}
            </button>
          </div>
          {discountError && <p className="mt-2 text-sm text-rcommerce-danger">{discountError}</p>}
        </div>
      )}

      <div className="mt-6 pt-6 border-t border-rcommerce-200 space-y-2.5 text-sm">
        <div className="flex justify-between">
          <span className="text-rcommerce-600">Subtotal</span>
          <span className="font-medium text-rcommerce-900">{currency} {formatMoney(subtotal)}</span>
        </div>
        {parseFloat(discount || '0') > 0 && (
          <div className="flex justify-between">
            <span className="text-rcommerce-600">Discount</span>
            <span className="font-medium text-rcommerce-success">−{currency} {formatMoney(discount)}</span>
          </div>
        )}
        <div className="flex justify-between">
          <span className="text-rcommerce-600">Shipping</span>
          {shipping != null ? (
            <span className="font-medium text-rcommerce-900">{currency} {formatMoney(shipping)}</span>
          ) : (
            <span className="text-rcommerce-500 text-[13px]">Calculated at next step</span>
          )}
        </div>
        <div className="flex justify-between">
          <span className="text-rcommerce-600">Taxes</span>
          <span className="font-medium text-rcommerce-900">{currency} {formatMoney(tax)}</span>
        </div>
      </div>

      <div className="flex justify-between items-baseline pt-5 mt-5 border-t border-rcommerce-200">
        <span className="text-base font-semibold text-rcommerce-900">Total</span>
        <span className="flex items-baseline gap-2">
          <span className="text-sm text-rcommerce-500">{currency}</span>
          <span className="font-serif text-[26px] leading-none text-rcommerce-900">{formatMoney(total)}</span>
        </span>
      </div>
    </div>
  );
}

type PaymentMethodKey = 'card' | 'googlepay' | 'applepay' | 'wechat' | 'alipay';

/**
 * Redirect methods (Alipay) navigate the browser away mid-payment. The entry
 * lets a returning/refreshing checkout page finalize the SAME order instead
 * of creating a new one. Key is written when the shopper enters the Alipay
 * step and cleared only when /checkout/confirm reports success.
 */
const PENDING_PAYMENT_KEY = 'rcommerce_pending_payment';
interface PendingPayment {
  order_id: string;
  payment_intent_id: string | null;
  cart_id: string;
  order_number: string;
  /** Set only when the customer navigated away to a provider (Alipay) — the
   *  one resume case where they may believe they already paid. */
  via?: 'redirect';
}

function readPendingPayment(): PendingPayment | null {
  try {
    const raw = sessionStorage.getItem(PENDING_PAYMENT_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    return parsed && parsed.order_id && parsed.cart_id ? parsed : null;
  } catch {
    return null;
  }
}

function writePendingPayment(entry: PendingPayment) {
  try {
    sessionStorage.setItem(PENDING_PAYMENT_KEY, JSON.stringify(entry));
  } catch {
    // storage unavailable — resume-on-return just won't be possible
  }
}

function clearPendingPayment() {
  try {
    sessionStorage.removeItem(PENDING_PAYMENT_KEY);
  } catch {
    // ignore
  }
}

/// The confirm endpoint answers with "not succeeded"-style errors while the
/// intent is still pending (webhook hasn't landed). Those are NOT failures —
/// the payment may still complete, so the UI must go to the "processing"
/// notice rather than any "failed, do something" message.
function isPendingFailure(message: string | undefined): boolean {
  return !!message && /not succeeded|pending|processing/i.test(message);
}

/// Hosted-field styling approximating coInput (iframes can't inherit CSS).
const AWX_FIELD_STYLE: Payment.InputStyle = {
  base: {
    border: '1px solid #c9c5bc',
    borderRadius: '6px',
    padding: '12px 14px',
    fontSize: '15px',
    lineHeight: '22px',
    color: '#161616',
    fontFamily: "'Work Sans', system-ui, sans-serif",
  },
};

function PaymentSection({
  summary,
  cart,
  address,
  email,
  onSelectShipping,
  onSuccess,
  onError,
  onVerificationRequired,
  airwallexEnv,
}: {
  summary: CheckoutSummary;
  cart: Cart;
  address: AddressForm;
  email: string;
  onSelectShipping: (rate: ShippingRate) => void;
  onSuccess: (result: CheckoutResult) => void;
  onError: (msg: string) => void;
  onVerificationRequired: () => void;
  airwallexEnv?: string;
}) {
  const [paymentResult, setPaymentResult] = useState<CheckoutResult | null>(null);
  const [initError, setInitError] = useState<string | null>(null);
  const [method, setMethod] = useState<PaymentMethodKey>('card');
  const [wallets, setWallets] = useState<{ google: boolean; apple: boolean }>({ google: false, apple: false });
  const [cardComplete, setCardComplete] = useState({ number: false, expiry: false, cvc: false });
  const [fieldsReady, setFieldsReady] = useState({ number: false, expiry: false, cvc: false });
  const [cardholderName, setCardholderName] = useState('');
  const [paying, setPaying] = useState(false);
  const [paymentError, setPaymentError] = useState<string | null>(null);
  const [processingNotice, setProcessingNotice] = useState<{ orderNumber: string } | null>(null);
  const [resumeNotice, setResumeNotice] = useState<string | null>(null);

  const paymentsRef = useRef<Payment.Airwallex | null>(null);
  const paymentResultRef = useRef<CheckoutResult | null>(null);
  const cartIdRef = useRef(cart.id);
  const finalizedRef = useRef(false);
  const elementsStartedRef = useRef(false);
  const observerRef = useRef<MutationObserver | null>(null);
  const pageHideHandlerRef = useRef<(() => void) | null>(null);
  const wechatStartedRef = useRef(false);
  const alipayStartedRef = useRef(false);
  const elementsRef = useRef<{
    cardNumber?: Payment.CardNumberElementType;
    google?: Payment.GooglePayButtonElementType;
    apple?: Payment.ApplePayButtonElementType;
    wechat?: Payment.DropInElementType;
    alipay?: Payment.DropInElementType;
  }>({});

  const cardNumberHostRef = useRef<HTMLDivElement>(null);
  const cardExpiryHostRef = useRef<HTMLDivElement>(null);
  const cardCvcHostRef = useRef<HTMLDivElement>(null);
  const googleHostRef = useRef<HTMLDivElement>(null);
  const appleHostRef = useRef<HTMLDivElement>(null);
  const wechatHostRef = useRef<HTMLDivElement>(null);
  const alipayHostRef = useRef<HTMLDivElement>(null);

  // The backend reports its Airwallex mode ('demo' when test_mode is on,
  // 'prod' otherwise) on the checkout summary; fall back to the build-time
  // default when the server does not provide it.
  const env: 'demo' | 'prod' =
    airwallexEnv === 'demo' || airwallexEnv === 'prod'
      ? airwallexEnv
      : AIRWALLEX_ENV === 'demo'
        ? 'demo'
        : 'prod';

  const postConfirm = (payload: { order_id: string; payment_intent_id: string | null; cart_id: string }) => {
    const checkoutFetch = apiFetchOptionalAuth;
    return checkoutFetch<CheckoutResult>('/checkout/confirm', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(payload),
    });
  };

  /// Step 4+7: finalize the order with ONE /checkout/confirm call. No polling,
  /// no re-created orders — the backend endpoint is safely replayable.
  const finalizePayment = async () => {
    const result = paymentResultRef.current;
    if (!result || finalizedRef.current) return;
    finalizedRef.current = true;
    setPaying(true);
    setPaymentError(null);

    try {
      const confirmed = await postConfirm({
        order_id: result.order.id,
        payment_intent_id: result.payment_intent_id,
        cart_id: cartIdRef.current,
      });
      clearPendingPayment();
      clearStoredCartId();
      onSuccess(confirmed?.order ? confirmed : result);
    } catch (e: any) {
      finalizedRef.current = false; // replayable — the shopper may try again
      if (e.status === 403 && e.code === 'email_verification_required') {
        onVerificationRequired();
      } else if (isPendingFailure(e.message)) {
        // Intent may still complete — do NOT offer a re-pay on a new order.
        setProcessingNotice({ orderNumber: result.order.order_number });
      } else {
        setPaymentError(e.message || 'Payment failed. Please try again.');
      }
    } finally {
      setPaying(false);
    }
  };

  // Init: resume an interrupted redirect payment first, then create the
  // intent (idempotent server-side — a replay returns the existing order)
  // and mount our own payment elements.
  useEffect(() => {
    let cancelled = false;
    const created: { destroy?: () => void }[] = [];
    const track = <T extends { destroy?: () => void }>(el: T): T => {
      created.push(el);
      return el;
    };

    const run = async () => {
      // Step 8: resume-on-return for redirect methods (e.g. Alipay). Exactly
      // one confirm attempt. The key survives everything except confirmed
      // success (or a definitively dead order).
      let resumeOrderNumber: string | null = null;
      let resumeViaRedirect = false;
      const pending = readPendingPayment();
      if (pending) {
        try {
          const confirmed = await postConfirm({
            order_id: pending.order_id,
            payment_intent_id: pending.payment_intent_id,
            cart_id: pending.cart_id,
          });
          if (cancelled) return;
          clearPendingPayment();
          clearStoredCartId();
          onSuccess(
            confirmed?.order
              ? confirmed
              : ({ ...confirmed, order: { id: pending.order_id, order_number: pending.order_number } } as CheckoutResult),
          );
          return;
        } catch (e: any) {
          if (cancelled) return;
          if (e.status === 403 && e.code === 'email_verification_required') {
            clearPendingPayment();
            onVerificationRequired();
          } else if (isPendingFailure(e.message)) {
            // The stored order is still unpaid — do NOT dead-end on a
            // "processing" notice. Fall through: the idempotent
            // /checkout/complete below returns the SAME pending order +
            // intent for this cart, and the form mounts so the customer can
            // actually pay. The key stays until confirmed success. The
            // "awaiting payment" notice only renders when the customer was
            // actually sent to a provider and back (via: 'redirect').
            resumeOrderNumber = pending.order_number;
            resumeViaRedirect = pending.via === 'redirect';
          } else {
            // Any other error means the stored payment is unusable — drop it
            // and fall through to a normal checkout.
            clearPendingPayment();
          }
        }
      }

      try {
        const selectedRate = summary.selected_shipping_rate || summary.available_shipping_rates[0];
        if (!selectedRate) {
          setInitError('Please select a shipping method.');
          return;
        }

        const checkoutFetch = apiFetchOptionalAuth;
        const result = await checkoutFetch<CheckoutResult>('/checkout/complete', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            cart_id: cart.id,
            shipping_address: address,
            billing_address: null,
            // The backend pays through its default gateway (Airwallex) regardless
            // of this tag; 'stripe_elements' only selects the client-side
            // confirmation flow and is ignored by the Airwallex gateway.
            payment_method: { type: 'stripe_elements' },
            customer_email: email,
            selected_shipping_rate: selectedRate,
          }),
        });

        if (cancelled) return;

        // Idempotent replay: the cart already has an order. When it's paid
        // (requires_confirmation false) we're done — no client_secret needed.
        if (!result.requires_confirmation || !result.client_secret) {
          clearPendingPayment();
          clearStoredCartId();
          onSuccess(result);
          return;
        }

        if (resumeOrderNumber && resumeViaRedirect && result.order?.order_number === resumeOrderNumber) {
          setResumeNotice(resumeOrderNumber);
        }

        paymentResultRef.current = result;
        setPaymentResult(result);
      } catch (err: any) {
        if (!cancelled) setInitError('Failed to load payment form: ' + (err.message || err));
      }
    };

    void run();

    return () => {
      cancelled = true;
    };
    // Runs once for the payment step; the intent is created per entry.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Create + mount the hosted elements once the intent exists. This MUST be
  // a separate effect keyed on paymentResult: effects run after React commits
  // the render, so the host refs below are guaranteed to be attached. Doing
  // this in the init effect raced React's commit — with warm script caches
  // initAirwallex() resolves in a microtask before the re-render commits, the
  // refs are still null, and element creation was silently skipped.
  useEffect(() => {
    if (!paymentResult || !paymentResult.client_secret || elementsStartedRef.current) return;
    elementsStartedRef.current = true;
    let cancelled = false;
    const created: { destroy?: () => void }[] = [];
    const track = <T extends { destroy?: () => void }>(el: T): T => {
      created.push(el);
      return el;
    };
    const result = paymentResult;

    const run = async () => {
      try {
        const { payments } = await initAirwallex({ env, enabledElements: ['payments'] });
        if (cancelled) return;
        paymentsRef.current = payments ?? null;

        if (!cardNumberHostRef.current || !cardExpiryHostRef.current || !cardCvcHostRef.current) {
          throw new Error('Card field containers are not mounted in the DOM.');
        }

        // Split card fields (Step 3) — validity tracked via 'change' events.
        const onFieldChange = (key: 'number' | 'expiry' | 'cvc') => (event: Payment.SplitElementEvent['change']) => {
          setCardComplete((prev) => ({ ...prev, [key]: event.detail.completed }));
        };
        const cardNumber = track(
          await createElement('cardNumber', {
            placeholder: 'Card number',
            style: AWX_FIELD_STYLE,
          }),
        );
        cardNumber.on('ready', () => setFieldsReady((prev) => ({ ...prev, number: true })));
        cardNumber.on('change', onFieldChange('number'));
        cardNumber.mount(cardNumberHostRef.current);
        elementsRef.current.cardNumber = cardNumber;

        const expiry = track(await createElement('expiry', { placeholder: 'MM / YY', style: AWX_FIELD_STYLE }));
        expiry.on('ready', () => setFieldsReady((prev) => ({ ...prev, expiry: true })));
        expiry.on('change', onFieldChange('expiry'));
        expiry.mount(cardExpiryHostRef.current);

        const cvc = track(await createElement('cvc', { placeholder: 'CVC', style: AWX_FIELD_STYLE }));
        cvc.on('ready', () => setFieldsReady((prev) => ({ ...prev, cvc: true })));
        cvc.on('change', onFieldChange('cvc'));
        cvc.mount(cardCvcHostRef.current);

        // The SDK's 'ready' event is not guaranteed for split fields — mark a
        // field ready when its iframe actually appears in the host element.
        const hosts: [HTMLDivElement, 'number' | 'expiry' | 'cvc'][] = [
          [cardNumberHostRef.current, 'number'],
          [cardExpiryHostRef.current, 'expiry'],
          [cardCvcHostRef.current, 'cvc'],
        ];
        const observer = new MutationObserver(() => {
          hosts.forEach(([host, key]) => {
            if (host.querySelector('iframe')) setFieldsReady((prev) => (prev[key] ? prev : { ...prev, [key]: true }));
          });
        });
        hosts.forEach(([host, key]) => {
          observer.observe(host, { childList: true });
          // mount() may have inserted the iframe before the observer attached
          if (host.querySelector('iframe')) setFieldsReady((prev) => (prev[key] ? prev : { ...prev, [key]: true }));
        });
        observerRef.current = observer;
      } catch (err: any) {
        console.error('[checkout] card element setup failed:', err);
        if (!cancelled) setInitError('Failed to load the card payment form: ' + (err?.message || err));
      }
    };
    void run();

    // Wallets (Step 4): created up front inside their (hidden) panels; a
    // failure hides the tab and is logged. Apple Pay only works on Safari.
    const amount = { value: parseFloat(summary.total) || 0, currency: summary.currency || 'USD' };
    const walletCountry = address.country || 'US';

    const setupGoogle = async () => {
      if (!googleHostRef.current) return;
      try {
        const google = track(
          await createElement('googlePayButton', {
            intent_id: result.payment_intent_id || undefined,
            client_secret: result.client_secret!,
            amount,
            countryCode: walletCountry,
          }),
        );
        google.on('success', () => {
          void finalizePayment();
        });
        google.on('error', (event) => {
          setWallets((prev) => ({ ...prev, google: false }));
          setPaymentError(event?.detail?.error?.message || 'Google Pay failed. Please try another method.');
        });
        google.mount(googleHostRef.current);
        elementsRef.current.google = google;
        setWallets((prev) => ({ ...prev, google: true }));
      } catch (err) {
        // Not offered by the shopper's browser/device — tab stays hidden.
        console.error('[checkout] Google Pay button unavailable:', err);
        setWallets((prev) => ({ ...prev, google: false }));
      }
    };

    const setupApple = async () => {
      if (!appleHostRef.current) return;
      try {
        const apple = track(
          await createElement('applePayButton', {
            intent_id: result.payment_intent_id || undefined,
            client_secret: result.client_secret!,
            amount,
            countryCode: walletCountry,
          }),
        );
        apple.on('success', () => {
          void finalizePayment();
        });
        apple.on('error', (event) => {
          setWallets((prev) => ({ ...prev, apple: false }));
          setPaymentError(event?.detail?.error?.message || 'Apple Pay failed. Please try another method.');
        });
        apple.mount(appleHostRef.current);
        elementsRef.current.apple = apple;
        setWallets((prev) => ({ ...prev, apple: true }));
      } catch (err) {
        // Documented not-available case (e.g. non-Safari) — tab stays hidden.
        console.error('[checkout] Apple Pay button unavailable:', err);
        setWallets((prev) => ({ ...prev, apple: false }));
      }
    };

    void setupGoogle();
    void setupApple();

    return () => {
      cancelled = true;
      observerRef.current?.disconnect();
      observerRef.current = null;
      if (pageHideHandlerRef.current) {
        window.removeEventListener('pagehide', pageHideHandlerRef.current);
        pageHideHandlerRef.current = null;
      }
      created.forEach((el) => el.destroy?.());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paymentResult]);

  // Lazy-start the APM drop-ins the first time their tab is shown. The
  // deprecated qrcode/redirect element types were removed from the live
  // Airwallex bundle (v2.38), so each APM gets a drop-in filtered to that
  // single method instead. Panels stay mounted (hidden), so the iframes
  // survive tab switches.
  useEffect(() => {
    const result = paymentResultRef.current;
    if (!result || !result.client_secret) return;
    const clientSecret = result.client_secret;
    const currency = summary.currency || 'USD';

    const startApmDropIn = async (
      key: 'wechat' | 'alipay',
      hostRef: RefObject<HTMLDivElement | null>,
      startedRef: { current: boolean },
      methods: Payment.PaymentMethodType[],
      label: string,
      beforeMount?: () => void,
    ) => {
      if (startedRef.current || !hostRef.current) return;
      startedRef.current = true;
      beforeMount?.();
      try {
        const dropIn = await createElement('dropIn', {
          client_secret: clientSecret,
          currency,
          intent_id: result.payment_intent_id || undefined,
          methods,
        });
        dropIn.on('success', () => {
          void finalizePayment();
        });
        dropIn.on('error', (event) => {
          setPaymentError(event?.detail?.error?.message || `${label} failed. Please try another method.`);
        });
        dropIn.mount(hostRef.current!);
        elementsRef.current[key] = dropIn;
      } catch (err: any) {
        console.error(`[checkout] ${label} drop-in failed:`, err);
        startedRef.current = false;
        setPaymentError(`${label} is unavailable right now. Please try another method.`);
      }
    };

    if (method === 'wechat') {
      void startApmDropIn('wechat', wechatHostRef, wechatStartedRef, ['wechatpay'], 'WeChat Pay');
    }
    if (method === 'alipay') {
      // Persist the pending-payment key only when the page is actually about
      // to navigate away to the provider. `pagehide` is the reliable
      // pre-navigation hook; the key is cleared on confirmed success
      // (finalize/resume paths).
      const pendingEntry: PendingPayment = {
        order_id: result.order.id,
        payment_intent_id: result.payment_intent_id,
        cart_id: cartIdRef.current,
        order_number: result.order.order_number,
        via: 'redirect',
      };
      const onPageHide = () => writePendingPayment(pendingEntry);
      void startApmDropIn('alipay', alipayHostRef, alipayStartedRef, ['alipaycn'], 'Alipay', () => {
        window.addEventListener('pagehide', onPageHide);
        pageHideHandlerRef.current = onPageHide;
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [method, paymentResult]);

  // Step 3: card Pay button — confirm the intent with the hosted fields.
  const handleCardPay = async () => {
    const result = paymentResultRef.current;
    const cardNumber = elementsRef.current.cardNumber;
    const payments = paymentsRef.current;
    if (!result || !cardNumber || !payments) return;
    setPaying(true);
    setPaymentError(null);
    try {
      const confirmed = await payments.confirmPaymentIntent({
        element: cardNumber as unknown as Payment.PaymentMethod['element'],
        intent_id: result.payment_intent_id || undefined,
        client_secret: result.client_secret || undefined,
        payment_method: { card: { name: cardholderName.trim() || undefined } },
      });
      if (confirmed === false) throw new Error('Your payment was not completed.');
      await finalizePayment();
    } catch (e: any) {
      // Only errors the SDK actually reports are surfaced here — while the
      // intent can still be processing we say nothing.
      setPaymentError(e.message || 'Payment failed. Please try again.');
    } finally {
      setPaying(false);
    }
  };

  const methodTabs: { key: PaymentMethodKey; label: string }[] = [
    { key: 'card', label: 'Card' },
    ...(wallets.google ? [{ key: 'googlepay' as const, label: 'Google Pay' }] : []),
    ...(wallets.apple ? [{ key: 'applepay' as const, label: 'Apple Pay' }] : []),
    { key: 'wechat', label: 'WeChat Pay' },
    { key: 'alipay', label: 'Alipay' },
  ];

  const cardReady = cardComplete.number && cardComplete.expiry && cardComplete.cvc;

  return (
    <div className="space-y-8">
      <section>
        <h2 className="text-[17px] font-semibold text-rcommerce-900 mb-3">Shipping method</h2>
        <div className="rounded-md border border-rcommerce-300 bg-white divide-y divide-rcommerce-200 overflow-hidden">
          {summary.available_shipping_rates.map((rate) => {
            const selected = summary.selected_shipping_rate?.service_code === rate.service_code;
            return (
              <div
                key={`${rate.carrier}-${rate.service_code}`}
                className={`flex items-center gap-3 px-4 py-3.5 ${selected ? 'bg-rcommerce-50' : ''}`}
              >
                <input
                  type="radio"
                  name="shipping"
                  checked={selected}
                  readOnly
                  className="accent-rcommerce-900"
                />
                <span className="flex-1 text-sm text-rcommerce-900">
                  {shippingRateLabel(rate)}
                </span>
                <span className="text-sm font-medium text-rcommerce-900">
                  {summary.currency} {formatMoney(rate.total_cost)}
                </span>
              </div>
            );
          })}
        </div>
      </section>

      <section>
        <h2 className="text-[17px] font-semibold text-rcommerce-900 mb-1">Payment</h2>
        <p className="text-sm text-rcommerce-500 mb-3">All transactions are secure and encrypted.</p>

        {processingNotice ? (
          <div className="rounded-md border border-rcommerce-300 bg-rcommerce-50 p-4">
            <p className="text-sm text-rcommerce-900">
              Your payment is being processed — we'll email your confirmation shortly. Order {processingNotice.orderNumber}.
            </p>
          </div>
        ) : (
          <div className="rounded-md border border-rcommerce-300 bg-white overflow-hidden">
            <div className="px-4 py-3 border-b border-rcommerce-200 bg-rcommerce-50 flex items-center justify-between">
              <span className="text-sm font-medium text-rcommerce-900">Payment methods</span>
              <span className="text-xs text-rcommerce-500 tracking-wide">Powered by Airwallex</span>
            </div>
            {initError ? (
              <p className="p-4 text-sm text-rcommerce-danger">{initError}</p>
            ) : !paymentResult ? (
              <p className="p-4 text-sm text-rcommerce-500">Loading payment form…</p>
            ) : (
              <div className="p-4 space-y-4">
                {resumeNotice && (
                  <p className="text-sm text-rcommerce-700 bg-rcommerce-50 border border-rcommerce-200 rounded-md px-3 py-2">
                    Your previous payment wasn't completed — you can finish it here. Order {resumeNotice}.
                  </p>
                )}
                <div className="rounded-md border border-rcommerce-300 divide-y divide-rcommerce-200 overflow-hidden">
                  {methodTabs.map((tab) => (
                    <button
                      key={tab.key}
                      type="button"
                      onClick={() => {
                        setMethod(tab.key);
                        setPaymentError(null);
                      }}
                      className={`flex w-full items-center gap-3 px-4 py-3 text-left ${method === tab.key ? 'bg-rcommerce-50' : 'bg-white'}`}
                    >
                      <input
                        type="radio"
                        name="payment-method"
                        checked={method === tab.key}
                        readOnly
                        className="accent-rcommerce-900"
                      />
                      <span className="flex-1 text-sm font-medium text-rcommerce-900">{tab.label}</span>
                    </button>
                  ))}
                </div>

                {/* Panels stay mounted in the DOM and toggle visibility with
                    `hidden` — unmounting would destroy the hosted iframes and
                    lose entered card data on every tab switch. WeChat/Alipay
                    elements still mount lazily on first selection. */}
                <div className={method === 'card' ? 'space-y-3' : 'space-y-3 hidden'}>
                  <div ref={cardNumberHostRef} className="min-h-[46px]" />
                  <div className="grid grid-cols-2 gap-3">
                    <div ref={cardExpiryHostRef} className="min-h-[46px]" />
                    <div ref={cardCvcHostRef} className="min-h-[46px]" />
                  </div>
                  {!fieldsReady.number && !initError && (
                    <p className="text-sm text-rcommerce-500">Loading card fields…</p>
                  )}
                  <label htmlFor="cardholder-name" className="sr-only">Name on card</label>
                  <input
                    id="cardholder-name"
                    type="text"
                    placeholder="Name on card"
                    autoComplete="cc-name"
                    value={cardholderName}
                    onChange={(e) => setCardholderName(e.target.value)}
                    className={coInput}
                  />
                  <button
                    type="button"
                    onClick={() => void handleCardPay()}
                    disabled={paying || !cardReady || !cardholderName.trim()}
                    className={'w-full ' + coBtn}
                  >
                    {paying ? 'Processing…' : `Pay ${summary.currency} ${formatMoney(summary.total)}`}
                  </button>
                </div>

                <div className={method === 'googlepay' ? 'space-y-3' : 'space-y-3 hidden'}>
                  <div ref={googleHostRef} className="min-h-[48px]" />
                  <p className="text-xs text-rcommerce-500">Confirm the total with Google Pay to complete your order.</p>
                </div>

                <div className={method === 'applepay' ? 'space-y-3' : 'space-y-3 hidden'}>
                  <div ref={appleHostRef} className="min-h-[48px]" />
                  <p className="text-xs text-rcommerce-500">Confirm the total with Apple Pay to complete your order.</p>
                </div>

                <div className={method === 'wechat' ? 'space-y-3' : 'space-y-3 hidden'}>
                  <div ref={wechatHostRef} className="min-h-[200px] flex items-center justify-center" />
                  <p className="text-xs text-rcommerce-500">Scan the QR code with WeChat to complete your order.</p>
                </div>

                <div className={method === 'alipay' ? 'space-y-3' : 'space-y-3 hidden'}>
                  <div ref={alipayHostRef} className="min-h-[48px]" />
                  <p className="text-xs text-rcommerce-500">
                    You'll be redirected to Alipay to complete your order, then returned here.
                  </p>
                </div>

                {paymentError && <p className="text-sm text-rcommerce-danger" role="alert">{paymentError}</p>}
                {paying && <p className="text-sm text-rcommerce-500 text-center">Confirming your payment…</p>}
              </div>
            )}
          </div>
        )}
      </section>
    </div>
  );
}

export default function CheckoutForm() {
  const [cart, setCart] = useState<Cart | null>(null);
  const [summary, setSummary] = useState<CheckoutSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [verificationRequired, setVerificationRequired] = useState(false);
  const [verificationSent, setVerificationSent] = useState(false);
  const [verificationLoading, setVerificationLoading] = useState(false);
  const [step, setStep] = useState<'information' | 'payment' | 'success'>('information');
  const [completedOrder, setCompletedOrder] = useState<CheckoutResult | null>(null);
  const [airwallexEnv, setAirwallexEnv] = useState<string | undefined>();
  // Shippable country codes from the API; null = fetch failed/loading (full
  // country list is offered as a fallback so checkout is never blocked).
  const [shippableCountries, setShippableCountries] = useState<string[] | null>(null);
  const [customer, setCustomer] = useState<Customer | null>(null);

  useEffect(() => {
    let cancelled = false;
    apiFetchJson<{ countries: string[] }>('/shipping/countries')
      .then((data) => {
        if (!cancelled && Array.isArray(data.countries)) setShippableCountries(data.countries);
      })
      .catch(() => {
        // Leave the full country list in place on failure.
      });
    return () => {
      cancelled = true;
    };
  }, []);
  const [discountCode, setDiscountCode] = useState('');
  const [discountLoading, setDiscountLoading] = useState(false);
  const [discountError, setDiscountError] = useState<string | null>(null);

  const [email, setEmail] = useState('');
  const [address, setAddress] = useState<AddressForm>(emptyAddress);

  useEffect(() => {
    getOrCreateCart().then(setCart);
    apiFetchAuthenticated<{ customer: Customer; addresses: AddressForm[] }>('/customers/me')
      .then((data) => {
        setCustomer(data.customer);
        setEmail(data.customer.email || '');
        if (data.addresses && data.addresses.length > 0) {
          const defaultShipping = data.addresses.find((a) => a.is_default_shipping) || data.addresses[0];
          setAddress((prev) => ({
            ...prev,
            first_name: defaultShipping.first_name || data.customer.first_name || prev.first_name,
            last_name: defaultShipping.last_name || data.customer.last_name || prev.last_name,
            address1: defaultShipping.address1 || prev.address1,
            address2: defaultShipping.address2 ?? prev.address2,
            city: defaultShipping.city || prev.city,
            state: defaultShipping.state || prev.state,
            country: defaultShipping.country || prev.country,
            zip: defaultShipping.zip || prev.zip,
            phone: defaultShipping.phone || data.customer.phone || prev.phone,
            company: defaultShipping.company ?? prev.company,
            is_default_shipping: defaultShipping.is_default_shipping,
            is_default_billing: defaultShipping.is_default_billing,
          }));
        } else {
          setAddress((prev) => ({
            ...prev,
            first_name: data.customer.first_name || prev.first_name,
            last_name: data.customer.last_name || prev.last_name,
            phone: data.customer.phone || prev.phone,
          }));
        }
      })
      .catch(() => {
        setCustomer(null);
      });
  }, []);

  const initiateCheckout = async (currentCart: Cart, currentAddress: AddressForm) => {
    const res = await apiFetchOptionalAuth<CheckoutSummary>('/checkout/initiate', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        cart_id: currentCart.id,
        shipping_address: currentAddress,
        billing_address: null,
        currency: currentCart.currency,
      }),
    });
    setSummary(res);
    if (res.airwallex_env) {
      setAirwallexEnv(res.airwallex_env);
    }
    return res;
  };

  const handleInformationSubmit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!cart) return;

    if (
      !email.trim() ||
      !address.first_name.trim() ||
      !address.last_name.trim() ||
      !address.address1.trim() ||
      !address.city.trim() ||
      !address.zip.trim() ||
      !address.country.trim() ||
      !address.phone.trim()
    ) {
      setError('Please fill in all required fields, including a phone number for the carrier.');
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await initiateCheckout(cart, address);
      setStep('payment');
    } catch (e: any) {
      setError(e.message || 'Failed to initiate checkout');
    } finally {
      setLoading(false);
    }
  };

  const handleApplyDiscount = async () => {
    if (!cart || !discountCode.trim()) return;
    setDiscountLoading(true);
    setDiscountError(null);
    try {
      await applyCoupon(cart.id, discountCode.trim());
      const fresh = await getOrCreateCart();
      setCart(fresh);
      setDiscountCode('');
      if (step === 'payment') {
        await initiateCheckout(fresh, address);
      }
    } catch (e: any) {
      setDiscountError(e.message || 'Invalid discount code');
    } finally {
      setDiscountLoading(false);
    }
  };

  const selectShipping = async (rate: ShippingRate) => {
    if (!cart || !summary) return;
    setLoading(true);
    try {
      const res = await apiFetchOptionalAuth<CheckoutSummary>('/checkout/shipping', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          cart_id: cart.id,
          shipping_address: address,
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

  const handleResendVerification = async () => {
    if (!customer?.email) return;
    setVerificationLoading(true);
    setError(null);
    try {
      await apiFetchJson('/auth/resend-verification', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ email: customer.email }),
      });
      setVerificationSent(true);
    } catch (e: any) {
      setError(e.message || 'Failed to resend verification email');
    } finally {
      setVerificationLoading(false);
    }
  };

  const addressLine = [address.address1, address.address2, `${address.city}${address.state ? `, ${address.state}` : ''} ${address.zip}`.trim(), address.country]
    .filter(Boolean)
    .join(', ');

  const logo = (
    <a href="/" className="inline-flex items-center" aria-label={`${brand.name} home`}>
      <img src={brand.logo} alt={brand.name} className="h-7 w-auto" />
    </a>
  );

  if (step === 'success') {
    const order = completedOrder?.order;
    const shippingRate = summary?.selected_shipping_rate || summary?.available_shipping_rates[0];
    const fullName = [address.first_name, address.last_name].filter(Boolean).join(' ');

    return (
      <div className="flex flex-col lg:flex-row min-h-screen bg-white">
        <div className="flex-1 order-1">
          <div className="max-w-[620px] mx-auto lg:mx-0 lg:ml-auto px-4 sm:px-8 lg:pl-8 lg:pr-14 py-8 lg:py-10">
            <div className="mb-10">{logo}</div>

            <div className="flex items-start gap-4">
              <span className="inline-flex w-12 h-12 shrink-0 items-center justify-center rounded-full bg-rcommerce-success/10 text-rcommerce-success">
                <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              </span>
              <div>
                <p className="eyebrow text-lg">Order confirmed</p>
                <h1 className="font-serif text-3xl md:text-4xl font-semibold text-rcommerce-900 mt-0.5">
                  Thank you{address.first_name ? `, ${address.first_name}` : ''}!
                </h1>
              </div>
            </div>

            <div className="mt-8 rounded-md border border-rcommerce-300 bg-white overflow-hidden">
              <div className="px-4 py-3 border-b border-rcommerce-200 bg-rcommerce-50 flex items-center justify-between">
                <span className="text-sm font-medium text-rcommerce-900">
                  Order {order?.order_number || ''}
                </span>
                {order?.status && (
                  <span className="text-xs capitalize bg-rcommerce-100 text-rcommerce-700 px-2 py-0.5 rounded-full">
                    {order.status}
                  </span>
                )}
              </div>
              <div className="px-4 py-4 grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-5 text-sm">
                <div>
                  <p className="text-rcommerce-500 mb-1">Contact information</p>
                  <p className="text-rcommerce-900">{email}</p>
                  {address.phone && <p className="text-rcommerce-900 mt-0.5">{address.phone}</p>}
                </div>
                <div>
                  <p className="text-rcommerce-500 mb-1">Shipping address</p>
                  <p className="text-rcommerce-900">{fullName}</p>
                  <p className="text-rcommerce-900">{address.address1}{address.address2 ? `, ${address.address2}` : ''}</p>
                  <p className="text-rcommerce-900">
                    {address.city}{address.state ? `, ${address.state}` : ''} {address.zip}, {address.country}
                  </p>
                </div>
                <div>
                  <p className="text-rcommerce-500 mb-1">Shipping method</p>
                  <p className="text-rcommerce-900">
                    {shippingRate ? shippingRateLabel(shippingRate) : 'Standard'}
                  </p>
                </div>
                <div>
                  <p className="text-rcommerce-500 mb-1">Payment method</p>
                  <p className="text-rcommerce-900">Secure online payment · Airwallex</p>
                </div>
              </div>
            </div>

            <p className="mt-6 text-sm text-rcommerce-600">
              A confirmation email is on its way{email ? ` to ${email}` : ''}. You can track this order any time from your account.
            </p>

            <div className="mt-8 flex flex-wrap gap-4">
              <a href="/account" className={coBtn + ' !bg-white !text-rcommerce-900 border border-rcommerce-300 hover:!bg-rcommerce-50'}>View orders</a>
              <a href="/products" className={coBtn}>Continue shopping</a>
            </div>
          </div>
        </div>

        {cart && (
          <aside className="order-2 bg-rcommerce-50 border-t lg:border-t-0 lg:border-l border-rcommerce-200 lg:w-[44%] xl:w-[42%]">
            <OrderSummary
              cart={cart}
              summary={summary}
              onApplyDiscount={() => {}}
              discountCode=""
              setDiscountCode={() => {}}
              discountLoading={false}
              discountError={null}
              showDiscountField={false}
            />
          </aside>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col lg:flex-row min-h-screen bg-white">
      {/* Main column */}
      <div className="flex-1 order-1">
        <div className="max-w-[620px] mx-auto lg:mx-0 lg:ml-auto px-4 sm:px-8 lg:pl-8 lg:pr-14 py-8 lg:py-10">
          <div className="flex items-center justify-between mb-6">
            {logo}
            <a href="/cart" aria-label="Return to cart" className="p-2 text-rcommerce-600 hover:text-rcommerce-accent transition-colors">
              <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                <path d="M6 7h12l1.5 13.5a1 1 0 0 1-1 1.1H5.5a1 1 0 0 1-1-1.1L6 7Z" />
                <path d="M9 10V6a3 3 0 0 1 6 0v4" />
              </svg>
            </a>
          </div>

          <nav className="flex items-center gap-2 text-[13px] mb-8" aria-label="Checkout steps">
            <a href="/cart" className="text-rcommerce-accent hover:underline">Cart</a>
            <span className="text-rcommerce-400">›</span>
            <span className={step === 'information' ? 'text-rcommerce-900 font-medium' : 'text-rcommerce-600'}>Information</span>
            <span className="text-rcommerce-400">›</span>
            <span className={step === 'payment' ? 'text-rcommerce-900 font-medium' : 'text-rcommerce-400'}>Shipping &amp; payment</span>
          </nav>

          {step === 'information' && (
            <form onSubmit={handleInformationSubmit} className="space-y-8">
              <section>
                <div className="flex items-center justify-between mb-3">
                  <h2 className="text-[17px] font-semibold text-rcommerce-900">Contact</h2>
                  {!customer && (
                    <a href="/account/login?redirect=/checkout" className="text-sm text-rcommerce-accent hover:underline">
                      Log in
                    </a>
                  )}
                </div>
                <label htmlFor="email" className="sr-only">Email</label>
                <input
                  id="email"
                  name="email"
                  type="email"
                  placeholder="Email"
                  required
                  autoComplete="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  className={coInput}
                />
              </section>

              <section>
                <h2 className="text-[17px] font-semibold text-rcommerce-900 mb-3">Shipping address</h2>
                <div className="space-y-3">
                  <div className="grid grid-cols-2 gap-3">
                    <input placeholder="First name" required autoComplete="given-name" value={address.first_name} onChange={(e) => setAddress({ ...address, first_name: e.target.value })} className={coInput} />
                    <input placeholder="Last name" required autoComplete="family-name" value={address.last_name} onChange={(e) => setAddress({ ...address, last_name: e.target.value })} className={coInput} />
                  </div>
                  <input placeholder="Address" required autoComplete="street-address" value={address.address1} onChange={(e) => setAddress({ ...address, address1: e.target.value })} className={coInput} />
                  <input placeholder="Apartment, suite, etc. (optional)" autoComplete="address-line2" value={address.address2 ?? ''} onChange={(e) => setAddress({ ...address, address2: e.target.value })} className={coInput} />
                  <div className="grid grid-cols-2 gap-3">
                    <input placeholder="City" required autoComplete="address-level2" value={address.city} onChange={(e) => setAddress({ ...address, city: e.target.value })} className={coInput} />
                    <input placeholder="Postal code" required autoComplete="postal-code" value={address.zip} onChange={(e) => setAddress({ ...address, zip: e.target.value })} className={coInput} />
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <CountryStateFields
                      country={address.country}
                      onCountryChange={(value) => setAddress({ ...address, country: value })}
                      state={address.state}
                      onStateChange={(value) => setAddress({ ...address, state: value })}
                      className={coInput}
                      shippable={shippableCountries}
                      countryAutoComplete="country"
                      stateAutoComplete="address-level1"
                    />
                  </div>
                  <input type="tel" placeholder="Phone" required autoComplete="tel" value={address.phone} onChange={(e) => setAddress({ ...address, phone: e.target.value })} className={coInput} />
                </div>
              </section>

              {error && <p className="text-rcommerce-danger text-sm" aria-live="polite">{error}</p>}

              <button type="submit" disabled={loading} className={'w-full ' + coBtn}>
                {loading ? 'Calculating…' : 'Continue to shipping'}
              </button>
            </form>
          )}

          {step === 'payment' && summary && (
            <div className="space-y-8">
              <div className="rounded-md border border-rcommerce-300 bg-white overflow-hidden">
                <ReviewRow label="Contact" value={email} onChange={() => setStep('information')} />
                <ReviewRow label="Ship to" value={addressLine} onChange={() => setStep('information')} />
              </div>

              {verificationRequired && (
                <div className="rounded-md bg-rcommerce-highlight/20 border border-rcommerce-highlight p-4 space-y-3">
                  <p className="text-sm text-rcommerce-900">
                    Please verify your email address before completing your order.
                  </p>
                  {verificationSent ? (
                    <p className="text-sm text-rcommerce-success">A verification link has been sent to {customer?.email}.</p>
                  ) : (
                    <button
                      type="button"
                      onClick={handleResendVerification}
                      disabled={verificationLoading}
                      className="rounded-md bg-white border border-rcommerce-300 px-4 py-2 text-sm font-medium text-rcommerce-900 hover:bg-rcommerce-50"
                    >
                      {verificationLoading ? 'Sending…' : 'Resend verification email'}
                    </button>
                  )}
                </div>
              )}

              {cart && summary ? (
                <PaymentSection
                  summary={summary}
                  cart={cart}
                  address={address}
                  email={email}
                  onSelectShipping={selectShipping}
                  onSuccess={(result) => { setCompletedOrder(result); setStep('success'); }}
                  onError={(msg) => { setError(msg); setVerificationRequired(false); }}
                  onVerificationRequired={() => setVerificationRequired(true)}
                  airwallexEnv={airwallexEnv}
                />
              ) : (
                <p className="text-rcommerce-500 text-sm">Loading payment form…</p>
              )}

              {error && <p className="text-rcommerce-danger text-sm" aria-live="polite">{error}</p>}
            </div>
          )}
        </div>
      </div>

      {/* Summary sidebar */}
      <aside className="order-2 bg-rcommerce-50 border-t lg:border-t-0 lg:border-l border-rcommerce-200 lg:w-[44%] xl:w-[42%]">
        {cart ? (
          <OrderSummary
            cart={cart}
            summary={summary}
            onApplyDiscount={handleApplyDiscount}
            discountCode={discountCode}
            setDiscountCode={setDiscountCode}
            discountLoading={discountLoading}
            discountError={discountError}
          />
        ) : (
          <div className="px-8 py-12">
            <p className="text-sm text-rcommerce-500">Loading summary…</p>
          </div>
        )}
      </aside>
    </div>
  );
}
