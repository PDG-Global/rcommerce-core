# Store Locations API

Store locations are the branches and pickup points customers see. Each one has an address, contact details, services, weekly opening hours and special days (bank holidays, early closing). For any location, the API tells a storefront:
- whether it is open now, and when it closes or next opens;
- when an order placed now would be ready to collect.

A store location is an inventory location (`inventory_locations`) marked `is_public`. Existing locations stay private, so stores that don't use this see no change.

## Base URL

```
/api/v1/locations          (storefront)
/api/v1/admin/locations    (management, admin token)
```

## Storefront

### List locations

```http
GET /api/v1/locations?lang=nl&near=50.41,4.52
```

Returns public, active locations in display order (`sort_order`, then name).

| Parameter | Meaning |
|---|---|
| `lang` | Language for the translated description and pickup instructions (`nl`, `fr-BE`…). Without a translation, those fields are `null`. |
| `near` | `latitude,longitude`. Adds `distance_km` to each location and sorts nearest first. Invalid values are ignored. |

```json
[
  {
    "id": "6f1c…",
    "name": "Central",
    "code": "CEN",
    "slug": "central",
    "is_pickup_point": true,
    "phone": "+32 2 000 00 00",
    "email": "central@example.com",
    "address_line1": "Rue de l'Exemple 1",
    "postal_code": "1000",
    "city": "Brussels",
    "country_code": "BE",
    "latitude": 50.8467,
    "longitude": 4.3525,
    "timezone": "Europe/Brussels",
    "pickup_lead_minutes": 60,
    "services": ["pickup", "delivery", "showroom"],
    "manager_name": null,
    "description": "Bouwmaterialen en showroom",
    "pickup_instructions": null,
    "locale": "nl",
    "opening_hours": [
      { "weekday": 1, "opens_at": "07:00:00", "closes_at": "18:00:00" },
      { "weekday": 6, "opens_at": "08:00:00", "closes_at": "12:00:00" }
    ],
    "special_days": [
      { "id": "…", "date": "2026-11-11", "opens_at": null, "closes_at": null, "reason": "Armistice" }
    ],
    "status": { "is_open": true, "closes_at": "2026-10-27T17:00:00Z", "next_opening": null },
    "pickup_ready_at": "2026-10-27T10:15:00Z",
    "distance_km": 41.5
  }
]
```

- **`opening_hours`** are in the location's local time. `weekday` runs from 1 (Monday) to 7 (Sunday), and a day can have several ranges (closed at lunch).
- **`special_days`** cover the next 31 days. A day with no times is closed all day; a day with times is open only during those ranges.
- **`status`** uses UTC instants:
  - when open, `closes_at` is the end of the current range;
  - when closed, `next_opening` is the start of the next range, looking up to 31 days ahead;
  - a location with no hours has neither.
- **`pickup_ready_at`** is set for pickup points only. It is the first moment the location is open after now plus `pickup_lead_minutes`, so an order placed in the evening is ready at the next opening.

Daylight-saving changes are handled through the IANA time zone: a 7:00 opening stays at 7:00 local time all year.

### One location

```http
GET /api/v1/locations/{id-or-slug}?lang=fr
```

Same fields as the list. Answers `404` for an unknown, private or inactive location.

## Management (admin)

All management routes need an admin access token. Customers get `403`.

| Route | Purpose |
|---|---|
| `GET /admin/locations` | Every location, private ones included |
| `POST /admin/locations` | Create a location (`201`) |
| `PUT /admin/locations/{id}` | Replace a location's details |
| `PUT /admin/locations/{id}/opening-hours` | Replace the weekly hours (`204`) |
| `POST /admin/locations/{id}/closures` | Add a closed day or special hours (`201`) |
| `DELETE /admin/locations/{id}/closures/{closure_id}` | Remove one (`204`) |
| `PUT /admin/locations/{id}/translations/{locale}` | Set the description and pickup instructions in one language (`204`) |
| `DELETE /admin/locations/{id}/translations/{locale}` | Remove a translation (`204`) |

### Location body

```json
{
  "name": "Central",
  "code": "CEN",
  "slug": "central",
  "is_active": true,
  "is_public": true,
  "is_pickup_point": true,
  "phone": "+32 2 000 00 00",
  "email": "central@example.com",
  "address_line1": "Rue de l'Exemple 1",
  "postal_code": "1000",
  "city": "Brussels",
  "country_code": "BE",
  "latitude": 50.8467,
  "longitude": 4.3525,
  "timezone": "Europe/Brussels",
  "pickup_lead_minutes": 60,
  "services": ["pickup", "delivery", "showroom"],
  "manager_name": "Alex Martin",
  "sort_order": 1
}
```

**Validation (`400` when broken):**
- `name` and `code` are required.
- `slug` uses lower-case letters and digits separated by hyphens.
- `timezone` must be an IANA name; it defaults to `UTC`.
- `country_code` is two upper-case letters.
- `pickup_lead_minutes` runs from 0 to 10,080 and defaults to 60.
- `services` holds at most 30 lower-case names (letters and `_`).
- Coordinates must be valid latitude and longitude.
- A duplicate `code` or `slug` is a `400`.

### Opening hours

```json
{
  "ranges": [
    { "weekday": 1, "opens_at": "07:00:00", "closes_at": "12:00:00" },
    { "weekday": 1, "opens_at": "13:00:00", "closes_at": "18:00:00" },
    { "weekday": 6, "opens_at": "08:00:00", "closes_at": "12:00:00" }
  ]
}
```

The list replaces all weekly hours. A weekday that isn't listed is closed. Ranges must close after they open and must not overlap within a day. Ranges past midnight are not supported.

### Special days

```json
{ "date": "2026-12-24", "opens_at": "08:00:00", "closes_at": "12:00:00", "reason": "Christmas Eve" }
{ "date": "2026-12-25", "reason": "Christmas" }
```

Give both times, or neither for a day closed all day. Several rows on the same date give several ranges.
