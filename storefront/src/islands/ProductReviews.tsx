import { useEffect, useState } from 'react';
import type { Review, ReviewSummary } from '../lib/types';
import { apiFetchAuthenticated, apiFetchJson, getAuthToken } from '../lib/api';
import { formatDate } from '../lib/date';

interface Props {
  productId: string;
  initialReviews: Review[];
  initialAverage: number;
  initialTotal: number;
}

function StarIcon({ filled, size = 18 }: { filled: boolean; size?: number }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      className={filled ? 'fill-rcommerce-accent' : 'fill-rcommerce-200'}
      aria-hidden="true"
    >
      <path d="M12 17.3l-6.2 3.6 1.7-7L2 9.2l7.1-.6L12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7z" />
    </svg>
  );
}

const inputClass =
  'w-full border border-rcommerce-300 bg-white px-4 py-3 text-sm focus:outline-none focus:border-rcommerce-900 focus:ring-1 focus:ring-rcommerce-900 placeholder:text-rcommerce-400';

export default function ProductReviews({ productId, initialReviews, initialAverage, initialTotal }: Props) {
  const [reviews, setReviews] = useState<Review[]>(initialReviews);
  const [average, setAverage] = useState(initialAverage);
  const [total, setTotal] = useState(initialTotal);
  const [authed, setAuthed] = useState(false);
  const [loginRedirect, setLoginRedirect] = useState('/account/login?redirect=');
  const [rating, setRating] = useState(0);
  const [hoverRating, setHoverRating] = useState(0);
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  useEffect(() => {
    setAuthed(!!getAuthToken());
    setLoginRedirect(`/account/login?redirect=${encodeURIComponent(window.location.pathname)}`);
  }, []);

  const refresh = async () => {
    const data = await apiFetchJson<ReviewSummary>(`/products/${productId}/reviews`);
    setReviews(data.reviews);
    setAverage(data.average_rating);
    setTotal(data.total_reviews);
    document.querySelectorAll('[data-review-average]').forEach((el) => {
      el.textContent = data.average_rating.toFixed(1);
    });
    document.querySelectorAll('[data-review-count]').forEach((el) => {
      el.textContent = String(data.total_reviews);
    });
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSuccess(null);
    if (rating < 1 || rating > 5) {
      setError('Please select a star rating.');
      return;
    }
    if (!content.trim()) {
      setError('Please write a few words about this product.');
      return;
    }
    setSubmitting(true);
    try {
      await apiFetchAuthenticated(`/products/${productId}/reviews`, {
        method: 'POST',
        body: JSON.stringify({ rating, title: title.trim() || undefined, content: content.trim() }),
      });
      setRating(0);
      setHoverRating(0);
      setTitle('');
      setContent('');
      setSuccess('Thanks! Your review has been posted.');
      await refresh();
    } catch (err: any) {
      setError(err.message || 'Failed to submit your review. Please try again.');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="grid grid-cols-1 lg:grid-cols-3 gap-10 lg:gap-16">
      <div className="lg:col-span-2">
        {total === 0 ? (
          <div className="py-10 text-center border border-dashed border-rcommerce-300">
            <p className="font-serif italic text-xl text-rcommerce-500">No reviews yet.</p>
            <p className="mt-2 text-sm text-rcommerce-600">
              {authed ? 'Be the first to share your experience.' : 'Log in to be the first to review this product.'}
            </p>
          </div>
        ) : (
          <>
            <p className="text-sm text-rcommerce-500 mb-6">
              {total} {total === 1 ? 'review' : 'reviews'}
            </p>
            <ul className="divide-y divide-rcommerce-200">
              {reviews.map((review) => (
                <li key={review.id} className="py-6 first:pt-0">
                  <div className="flex items-center gap-3 flex-wrap">
                    <span className="flex items-center gap-0.5">
                      {[1, 2, 3, 4, 5].map((n) => (
                        <StarIcon key={n} filled={n <= Math.round(review.rating)} />
                      ))}
                    </span>
                    <span className="font-semibold text-rcommerce-900">{review.author_name}</span>
                    <span className="text-sm text-rcommerce-400">{formatDate(review.created_at)}</span>
                  </div>
                  {review.title && (
                    <p className="mt-2 font-serif font-semibold text-rcommerce-900">{review.title}</p>
                  )}
                  <p className="mt-1.5 text-[15px] text-rcommerce-600 leading-relaxed whitespace-pre-line">{review.content}</p>
                </li>
              ))}
            </ul>
          </>
        )}
      </div>

      <div>
        <div className="bg-rcommerce-50 border border-rcommerce-200 p-6">
          <h3 className="font-serif text-xl font-semibold text-rcommerce-900">Customer reviews</h3>
          {total > 0 ? (
            <div className="mt-3 flex items-center gap-3">
              <span className="flex items-center gap-0.5">
                {[1, 2, 3, 4, 5].map((n) => (
                  <StarIcon key={n} filled={n <= Math.round(average)} size={20} />
                ))}
              </span>
              <span className="text-sm text-rcommerce-600">
                <span className="font-semibold text-rcommerce-900">{average.toFixed(1)}</span> out of 5
              </span>
            </div>
          ) : (
            <p className="mt-2 text-sm text-rcommerce-600">This product has not been rated yet.</p>
          )}

          {authed ? (
            <form onSubmit={handleSubmit} className="mt-6 space-y-4">
              <div>
                <label htmlFor="review-rating" className="block text-[12px] font-semibold uppercase tracking-button text-rcommerce-900 mb-2">
                  Your rating <span className="text-rcommerce-accent">*</span>
                </label>
                <div
                  id="review-rating"
                  className="flex items-center gap-1"
                  onMouseLeave={() => setHoverRating(0)}
                >
                  {[1, 2, 3, 4, 5].map((n) => (
                    <button
                      key={n}
                      type="button"
                      aria-label={`${n} star${n === 1 ? '' : 's'}`}
                      onClick={() => setRating(n)}
                      onMouseEnter={() => setHoverRating(n)}
                      className="p-1 -ml-1 first:ml-0 transition-transform hover:scale-110"
                    >
                      <StarIcon filled={n <= (hoverRating || rating)} size={26} />
                    </button>
                  ))}
                </div>
              </div>
              <div>
                <label htmlFor="review-title" className="block text-[12px] font-semibold uppercase tracking-button text-rcommerce-900 mb-2">
                  Review title <span className="font-normal normal-case tracking-normal text-rcommerce-400">(optional)</span>
                </label>
                <input
                  id="review-title"
                  type="text"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  maxLength={200}
                  placeholder="Sum it up"
                  className={inputClass}
                />
              </div>
              <div>
                <label htmlFor="review-content" className="block text-[12px] font-semibold uppercase tracking-button text-rcommerce-900 mb-2">
                  Your review <span className="text-rcommerce-accent">*</span>
                </label>
                <textarea
                  id="review-content"
                  value={content}
                  onChange={(e) => setContent(e.target.value)}
                  rows={5}
                  required
                  placeholder="How does it ride? What do you love about it?"
                  className={inputClass}
                />
              </div>
              {error && <p className="text-sm text-rcommerce-danger">{error}</p>}
              {success && <p className="text-sm text-rcommerce-success">{success}</p>}
              <button
                type="submit"
                disabled={submitting}
                className="w-full btn-primary disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {submitting ? 'Submitting...' : total > 0 ? 'Update your review' : 'Submit review'}
              </button>
            </form>
          ) : (
            <div className="mt-6">
              <p className="text-sm text-rcommerce-600 leading-relaxed">
                Share your experience with this product to help other riders.
              </p>
              <a
                href={loginRedirect}
                className="mt-4 inline-block btn-secondary w-full text-center"
              >
                Log in to write a review
              </a>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
