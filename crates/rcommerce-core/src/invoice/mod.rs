//! Invoice data types and PDF generation for order invoices.

use crate::{Error, Result};
use printpdf::{
    Color, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage,
    PdfSaveOptions, Point, Pt, RawImage, RawImageData, RawImageFormat, Rgb, TextItem,
    XObjectTransform,
};

const HELVETICA_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Helvetica-Regular.ttf");
const HELVETICA_BOLD: &[u8] = include_bytes!("../../assets/fonts/Helvetica-Bold.ttf");
const LOGO_PNG: &[u8] = include_bytes!("../../assets/88-logo.png");

#[derive(Debug, Clone)]
pub struct InvoiceData {
    pub order_number: String,
    pub order_date: String,
    pub status: String,
    pub currency: String,
    pub customer_email: String,
    pub billing: Option<InvoiceAddress>,
    pub shipping: Option<InvoiceAddress>,
    pub items: Vec<InvoiceItem>,
    pub subtotal: String,
    /// Discount amount formatted as money ("12.75"); None = no discount line
    pub discount_total: Option<String>,
    /// Coupon code that produced the discount ("SUMMER20"); display only
    pub coupon_code: Option<String>,
    pub shipping_total: String,
    pub tax_total: String,
    pub total: String,
    pub payment_method: String,
    pub shipping_method: Option<String>,
    /// Store brand name (invoice header); empty falls back to the default
    pub brand_name: String,
    /// Store support email (invoice footer); empty falls back to the default
    pub brand_email: String,
}

#[derive(Debug, Clone)]
pub struct InvoiceAddress {
    pub first_name: String,
    pub last_name: String,
    pub address_1: String,
    pub address_2: Option<String>,
    pub city: String,
    pub state: Option<String>,
    pub postcode: String,
    pub country: String,
    pub phone: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InvoiceItem {
    pub name: String,
    /// Variant/option selection, e.g. "Degree: 50" (None = no variant line)
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub quantity: i32,
    pub unit_price: String,
    pub total: String,
    /// Pre-formatted weight, e.g. "0.49kg" (None = no weight line)
    pub weight: Option<String>,
}

pub fn format_address(addr: &InvoiceAddress) -> Vec<String> {
    let mut lines = vec![format!("{} {}", addr.first_name, addr.last_name)];
    if let Some(ref addr2) = addr.address_2 {
        if !addr2.is_empty() {
            lines.push(addr2.clone());
        }
    }
    lines.push(addr.address_1.clone());
    let city_line = if let Some(ref state) = addr.state {
        format!("{}, {} {}", addr.city, state, addr.postcode)
    } else {
        format!("{} {}", addr.city, addr.postcode)
    };
    lines.push(city_line);
    lines.push(addr.country.clone());
    if let Some(ref phone) = addr.phone {
        if !phone.is_empty() {
            lines.push(phone.clone());
        }
    }
    lines
}

const PAGE_WIDTH_PT: f32 = 595.28;
const PAGE_HEIGHT_PT: f32 = 841.89;
const MARGIN: f32 = 50.0;
const RIGHT_EDGE: f32 = PAGE_WIDTH_PT - MARGIN;
/// Shared grid: Quantity column x (also Ship To / totals labels)
const QTY_X: f32 = 350.0;
/// Shared grid: Price column x (also totals values)
const PRICE_X: f32 = 450.0;
/// Right-hand blocks (company, meta) sit left of the Quantity grid to leave
/// room for long order numbers.
const COMPANY_X: f32 = 332.0;
/// All money amounts right-align at this x, a few pt in from the page edge.
const MONEY_RIGHT: f32 = RIGHT_EDGE - 6.0;

fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::Rgb(Rgb::new(r, g, b, None))
}

fn black() -> Color {
    rgb(0.1, 0.1, 0.1)
}

fn gray() -> Color {
    rgb(0.45, 0.45, 0.45)
}

fn light_gray() -> Color {
    rgb(0.82, 0.82, 0.82)
}

/// Rough estimate of rendered text width (points) for wrapping/alignment.
fn estimate_width(text: &str, size: f32, bold: bool) -> f32 {
    let factor = if bold { 0.56 } else { 0.52 };
    text.chars().count() as f32 * size * factor
}

/// Currency symbol for a currency code (falls back to the code itself).
fn currency_symbol(code: &str) -> &str {
    match code {
        "USD" | "AUD" | "CAD" | "NZD" | "SGD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" | "CNY" => "¥",
        "HKD" => "HK$",
        other => other,
    }
}

/// Humanize a shipping method slug: "standard_us" -> "Standard Us".
pub fn display_shipping_method(method: &str) -> String {
    method
        .replace('_', " ")
        .split_whitespace()
        .map(|word| {
            // All-caps short region words: "us" -> "US"
            if word.len() <= 2 && word.chars().all(|c| c.is_ascii_alphabetic()) {
                return word.to_uppercase();
            }
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn wrap_text(text: &str, size: f32, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };
        if estimate_width(&candidate, size, false) <= max_width || current.is_empty() {
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn write_text(ops: &mut Vec<Op>, font: PdfFontHandle, size: f32, x: f32, y: f32, text: &str) {
    ops.push(Op::StartTextSection);
    ops.push(Op::SetFont {
        font,
        size: Pt(size),
    });
    ops.push(Op::SetTextCursor {
        pos: Point { x: Pt(x), y: Pt(y) },
    });
    ops.push(Op::ShowText {
        items: vec![TextItem::Text(text.to_string())],
    });
    ops.push(Op::EndTextSection);
}

fn draw_line(ops: &mut Vec<Op>, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: Color) {
    ops.push(Op::SaveGraphicsState);
    ops.push(Op::SetOutlineColor { col: color });
    ops.push(Op::SetOutlineThickness { pt: Pt(thickness) });
    ops.push(Op::DrawLine {
        line: Line {
            points: vec![
                LinePoint {
                    p: Point {
                        x: Pt(x1),
                        y: Pt(y1),
                    },
                    bezier: false,
                },
                LinePoint {
                    p: Point {
                        x: Pt(x2),
                        y: Pt(y2),
                    },
                    bezier: false,
                },
            ],
            is_closed: false,
        },
    });
    ops.push(Op::RestoreGraphicsState);
}

/// Decode the embedded 88 logo PNG into a printpdf raster image (RGBA8).
fn load_logo_image() -> Result<RawImage> {
    let mut decoder = png::Decoder::new(LOGO_PNG);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::internal(format!("Failed to read logo PNG: {}", e)))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| Error::internal(format!("Failed to decode logo PNG: {}", e)))?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(buf.len() / 3 * 4);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(px);
                out.push(255);
            }
            out
        }
        other => {
            return Err(Error::internal(format!(
                "Unsupported logo color type: {:?}",
                other
            )))
        }
    };

    Ok(RawImage {
        pixels: RawImageData::U8(rgba),
        width: info.width as usize,
        height: info.height as usize,
        data_format: RawImageFormat::RGBA8,
        tag: b"88-logo".to_vec(),
    })
}

/// Generate a single-page A4 PDF invoice (mimics the WooCommerce reference).
pub fn generate_invoice_pdf(data: &InvoiceData) -> Result<Vec<u8>> {
    let mut font_warnings = Vec::new();
    let regular = ParsedFont::from_bytes(HELVETICA_REGULAR, 0, &mut font_warnings)
        .ok_or_else(|| Error::internal("Failed to parse embedded regular font"))?;
    let bold = ParsedFont::from_bytes(HELVETICA_BOLD, 0, &mut font_warnings)
        .ok_or_else(|| Error::internal("Failed to parse embedded bold font"))?;

    let mut doc = PdfDocument::new(&format!("Invoice {}", data.order_number));
    let regular_id = doc.add_font(&regular);
    let bold_id = doc.add_font(&bold);
    let regular = PdfFontHandle::External(regular_id);
    let bold = PdfFontHandle::External(bold_id);

    let logo = load_logo_image()?;
    let logo_id = doc.add_image(&logo);
    let logo_width_pt = 46.0f32;
    let logo_dpi = logo.width as f32 * 72.0 / logo_width_pt;
    let logo_height_pt = logo.height as f32 * 72.0 / logo_dpi;

    let mut ops: Vec<Op> = Vec::new();
    let mut y = PAGE_HEIGHT_PT - 55.0;

    // Row 1: logo (top left, sole brand element) | company block (top right)
    ops.push(Op::UseXobject {
        id: logo_id,
        transform: XObjectTransform {
            translate_x: Some(Pt(MARGIN)),
            translate_y: Some(Pt(y - logo_height_pt)),
            rotate: None,
            scale_x: None,
            scale_y: None,
            dpi: Some(logo_dpi),
        },
    });
    let brand_name = if data.brand_name.is_empty() {
        "rCommerce"
    } else {
        &data.brand_name
    };
    write_text(
        &mut ops,
        bold.clone(),
        10.5,
        COMPANY_X,
        y - 11.0,
        brand_name,
    );
    let company_lines = [
        "2301 Bayfield Building,",
        "99 Hennessy Road,",
        "Wanchai, Hong Kong",
    ];
    for (i, line) in company_lines.iter().enumerate() {
        write_text(
            &mut ops,
            regular.clone(),
            9.5,
            COMPANY_X,
            y - 25.0 - i as f32 * 13.0,
            line,
        );
    }

    // Row 2: INVOICE heading (left)
    let invoice_y = y - logo_height_pt - 42.0;
    write_text(&mut ops, bold.clone(), 21.0, MARGIN, invoice_y, "INVOICE");

    // Row 3: single shipping address directly under INVOICE (plain lines, no
    // label); the meta block sits to the right at the same vertical level.
    let address_top = invoice_y - 40.0;
    let address_lines = data
        .shipping
        .clone()
        .or_else(|| data.billing.clone())
        .map(|a| format_address(&a))
        .unwrap_or_else(|| vec!["No address provided".to_string()]);
    for (i, line) in address_lines.iter().enumerate() {
        write_text(
            &mut ops,
            regular.clone(),
            9.5,
            MARGIN,
            address_top - i as f32 * 13.0,
            line,
        );
    }
    let meta_label_x = COMPANY_X;
    let meta_value_x = 414.0;
    let invoice_date = chrono::Utc::now().format("%B %d, %Y").to_string();
    let payment_method = if data.payment_method.is_empty() {
        "Credit Card".to_string()
    } else {
        data.payment_method.clone()
    };
    let meta_rows: Vec<(&str, String)> = vec![
        ("Invoice Date:", invoice_date),
        ("Order Number:", data.order_number.clone()),
        ("Order Date:", data.order_date.clone()),
        ("Payment Method:", payment_method),
    ];
    for (i, (label, value)) in meta_rows.iter().enumerate() {
        let row_y = address_top - i as f32 * 14.0;
        write_text(&mut ops, regular.clone(), 9.5, meta_label_x, row_y, label);
        write_text(&mut ops, regular.clone(), 9.5, meta_value_x, row_y, value);
    }

    // Items table with a solid black header bar (white bold column titles)
    let bar_top = address_top - address_lines.len() as f32 * 13.0 - 22.0;
    let bar_height = 24.0;
    ops.push(Op::SetFillColor { col: black() });
    ops.push(Op::DrawRectangle {
        rectangle: printpdf::Rect {
            x: Pt(MARGIN),
            y: Pt(bar_top - bar_height),
            width: Pt(RIGHT_EDGE - MARGIN),
            height: Pt(bar_height),
            mode: Some(printpdf::PaintMode::Fill),
            winding_order: None,
        },
    });
    ops.push(Op::SetFillColor {
        col: Color::Rgb(Rgb::new(1.0, 1.0, 1.0, None)),
    });
    write_text(
        &mut ops,
        bold.clone(),
        9.5,
        MARGIN + 6.0,
        bar_top - 16.5,
        "Product",
    );
    write_text(
        &mut ops,
        bold.clone(),
        9.5,
        QTY_X,
        bar_top - 16.5,
        "Quantity",
    );
    let price_header = "Price";
    write_text(
        &mut ops,
        bold.clone(),
        9.5,
        MONEY_RIGHT - estimate_width(price_header, 9.5, true),
        bar_top - 16.5,
        price_header,
    );
    ops.push(Op::SetFillColor { col: black() });

    // Item row content sits slightly inset from the table edge (like the
    // padding inside the black header bar); Quantity stays on its grid column
    // and Price amounts right-align at the page edge.
    let item_x = MARGIN + 6.0;
    y = bar_top - bar_height - 18.0;
    for item in &data.items {
        if y < 200.0 {
            break;
        }
        let name_lines = wrap_text(&item.name, 9.5, QTY_X - item_x - 10.0);
        for (i, line) in name_lines.iter().enumerate() {
            write_text(
                &mut ops,
                regular.clone(),
                9.5,
                item_x,
                y - i as f32 * 12.5,
                line,
            );
        }
        let qty = format!("{}", item.quantity);
        write_text(&mut ops, regular.clone(), 9.5, QTY_X, y, &qty);
        let price = format!("{}{}", currency_symbol(&data.currency), item.total);
        write_text(
            &mut ops,
            regular.clone(),
            9.5,
            MONEY_RIGHT - estimate_width(&price, 9.5, false),
            y,
            &price,
        );

        // Fixed sub-line grid: the variant selection sits 11pt below the name
        // (shifting the SKU/weight slots down), the SKU slot is always
        // reserved, and the weight slot 10.5pt below that (also always
        // reserved, so every row has the same height whether or not the
        // individual lines are drawn).
        let name_bottom = y - (name_lines.len() - 1) as f32 * 12.5;
        let mut sub_y = name_bottom - 11.0;
        if let Some(ref variant) = item.variant_title {
            if !variant.is_empty() {
                write_text(&mut ops, regular.clone(), 8.0, item_x, sub_y, variant);
                sub_y -= 10.5;
            }
        }
        let sku_y = sub_y;
        if let Some(ref sku) = item.sku {
            if !sku.is_empty() {
                write_text(
                    &mut ops,
                    regular.clone(),
                    8.0,
                    item_x,
                    sku_y,
                    &format!("SKU: {}", sku),
                );
            }
        }
        let weight_y = sku_y - 10.5;
        if let Some(ref weight) = item.weight {
            if !weight.is_empty() {
                write_text(
                    &mut ops,
                    regular.clone(),
                    8.0,
                    item_x,
                    weight_y,
                    &format!("Weight: {}", weight),
                );
            }
        }
        y = weight_y - 12.0;
        draw_line(&mut ops, MARGIN, y, RIGHT_EDGE, y, 0.4, light_gray());
        y -= 12.0;
    }

    // Totals block under the Quantity/Price columns: bold labels left at the
    // Quantity column x, amounts right-aligned at the (slightly inset) money
    // column, one uniform row rhythm, thin rule under Subtotal, thick rules
    // snug above and below Total.
    let symbol = currency_symbol(&data.currency);
    let row_pitch = 17.0;
    y -= 8.0;

    write_text(&mut ops, bold.clone(), 9.5, QTY_X, y, "Subtotal");
    let subtotal_value = format!("{}{}", symbol, data.subtotal);
    write_text(
        &mut ops,
        regular.clone(),
        9.5,
        MONEY_RIGHT - estimate_width(&subtotal_value, 9.5, false),
        y,
        &subtotal_value,
    );
    draw_line(
        &mut ops,
        QTY_X,
        y - 6.0,
        MONEY_RIGHT,
        y - 6.0,
        0.5,
        light_gray(),
    );
    y -= row_pitch;

    // Discount row: only when a discount was applied, shown as a negative
    // amount in green, labelled with the coupon code when known.
    let discount_amount = data
        .discount_total
        .as_deref()
        .and_then(|d| d.parse::<rust_decimal::Decimal>().ok())
        .filter(|d| *d > rust_decimal::Decimal::ZERO);
    if let Some(discount) = discount_amount {
        let label = match data.coupon_code.as_deref() {
            Some(code) if !code.is_empty() => format!("Discount ({code})"),
            _ => "Discount".to_string(),
        };
        write_text(&mut ops, bold.clone(), 9.5, QTY_X, y, &label);
        let discount_value = format!("-{}{}", symbol, discount);
        ops.push(Op::SetFillColor {
            col: rgb(0.02, 0.59, 0.41), // #059669
        });
        write_text(
            &mut ops,
            regular.clone(),
            9.5,
            MONEY_RIGHT - estimate_width(&discount_value, 9.5, false),
            y,
            &discount_value,
        );
        ops.push(Op::SetFillColor { col: black() });
        y -= row_pitch;
    }

    write_text(&mut ops, bold.clone(), 9.5, QTY_X, y, "Shipping");
    let shipping_value = format!("{}{}", symbol, data.shipping_total);
    write_text(
        &mut ops,
        regular.clone(),
        9.5,
        MONEY_RIGHT - estimate_width(&shipping_value, 9.5, false),
        y,
        &shipping_value,
    );
    y -= row_pitch;

    if data.tax_total != "0.00" {
        write_text(&mut ops, bold.clone(), 9.5, QTY_X, y, "Tax");
        let tax_value = format!("{}{}", symbol, data.tax_total);
        write_text(
            &mut ops,
            regular.clone(),
            9.5,
            MONEY_RIGHT - estimate_width(&tax_value, 9.5, false),
            y,
            &tax_value,
        );
        y -= row_pitch;
    }

    y -= 6.0;
    draw_line(&mut ops, QTY_X, y, MONEY_RIGHT, y, 2.0, black());
    y -= 12.0;
    let total_value = format!("{}{}", symbol, data.total);
    write_text(&mut ops, bold.clone(), 10.5, QTY_X, y, "Total");
    write_text(
        &mut ops,
        bold.clone(),
        10.5,
        MONEY_RIGHT - estimate_width(&total_value, 10.5, true),
        y,
        &total_value,
    );
    y -= 12.0;
    draw_line(&mut ops, QTY_X, y, MONEY_RIGHT, y, 2.0, black());

    // Footer pinned to the bottom of the page, centered.
    let footer_rule_y = 100.0;
    draw_line(
        &mut ops,
        MARGIN,
        footer_rule_y,
        RIGHT_EDGE,
        footer_rule_y,
        0.4,
        light_gray(),
    );
    let brand_email = if data.brand_email.is_empty() {
        "support@example.com"
    } else {
        &data.brand_email
    };
    let contact = format!("Thank you for your order! Questions: {brand_email}");
    let footer = "Payments are in USD. All sales are final.";
    ops.push(Op::SetFillColor { col: gray() });
    write_text(
        &mut ops,
        regular.clone(),
        9.0,
        (PAGE_WIDTH_PT - estimate_width(&contact, 9.0, false)) / 2.0,
        footer_rule_y - 15.0,
        &contact,
    );
    write_text(
        &mut ops,
        regular.clone(),
        9.0,
        (PAGE_WIDTH_PT - estimate_width(footer, 9.0, false)) / 2.0,
        footer_rule_y - 29.0,
        footer,
    );
    ops.push(Op::SetFillColor { col: black() });

    doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));

    let mut save_warnings = Vec::new();
    Ok(doc.save(&PdfSaveOptions::default(), &mut save_warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_invoice() -> InvoiceData {
        InvoiceData {
            order_number: "ORD123456".to_string(),
            order_date: "October 01, 2026".to_string(),
            status: "completed".to_string(),
            currency: "USD".to_string(),
            customer_email: "customer@example.com".to_string(),
            billing: Some(InvoiceAddress {
                first_name: "John".to_string(),
                last_name: "Doe".to_string(),
                address_1: "123 Main Street".to_string(),
                address_2: None,
                city: "Los Angeles".to_string(),
                state: Some("CA".to_string()),
                postcode: "90001".to_string(),
                country: "United States".to_string(),
                phone: None,
            }),
            shipping: Some(InvoiceAddress {
                first_name: "John".to_string(),
                last_name: "Doe".to_string(),
                address_1: "123 Main Street".to_string(),
                address_2: Some("Apt 4B".to_string()),
                city: "Los Angeles".to_string(),
                state: Some("CA".to_string()),
                postcode: "90001".to_string(),
                country: "United States".to_string(),
                phone: Some("+1 555 123 4567".to_string()),
            }),
            items: vec![
                InvoiceItem {
                    name: "Example Premium Longboard Wheels 70mm 78a Orange".to_string(),
                    variant_title: None,
                    sku: Some("WHEEL-70-ORG".to_string()),
                    quantity: 2,
                    unit_price: "45.00".to_string(),
                    total: "90.00".to_string(),
                    weight: Some("0.49kg".to_string()),
                },
                InvoiceItem {
                    name: "Caliber 3 Precision Baseplate".to_string(),
                    variant_title: Some("Degree: 50".to_string()),
                    sku: Some("CALIBER3-50".to_string()),
                    quantity: 1,
                    unit_price: "75.00".to_string(),
                    total: "75.00".to_string(),
                    weight: None,
                },
                InvoiceItem {
                    name: "Skate Tool".to_string(),
                    variant_title: None,
                    sku: Some("TOOL-01".to_string()),
                    quantity: 1,
                    unit_price: "25.00".to_string(),
                    total: "25.00".to_string(),
                    weight: None,
                },
            ],
            subtotal: "115.00".to_string(),
            discount_total: None,
            coupon_code: None,
            shipping_total: "10.00".to_string(),
            tax_total: "0.00".to_string(),
            total: "125.00".to_string(),
            payment_method: "Credit Card".to_string(),
            shipping_method: Some("Standard US Shipping".to_string()),
            brand_name: String::new(),
            brand_email: String::new(),
        }
    }

    #[test]
    fn test_generate_invoice_pdf() {
        let pdf = generate_invoice_pdf(&sample_invoice()).expect("pdf generation failed");
        assert!(pdf.len() > 2000, "pdf too small: {}", pdf.len());
        assert_eq!(&pdf[..4], b"%PDF");
    }

    #[test]
    fn test_generate_invoice_pdf_with_variant_item() {
        let data = sample_invoice();
        let pdf = generate_invoice_pdf(&data).expect("pdf generation failed");
        assert_eq!(&pdf[..4], b"%PDF");
        // Variant item rendered without disturbing the rows around it; the
        // text is font-encoded, so we assert structural success only here.
        assert!(pdf.len() > 2000);
    }

    #[test]
    fn test_generate_invoice_pdf_with_discount() {
        let mut data = sample_invoice();
        data.discount_total = Some("12.75".to_string());
        data.coupon_code = Some("1V97NIBP40".to_string());
        data.total = "112.25".to_string();
        let pdf = generate_invoice_pdf(&data).expect("pdf generation failed");
        assert_eq!(&pdf[..4], b"%PDF");
        assert!(pdf.len() > 2000);
    }

    #[test]
    fn test_generate_invoice_pdf_zero_discount_no_line() {
        let mut data = sample_invoice();
        data.discount_total = Some("0.00".to_string());
        let pdf = generate_invoice_pdf(&data).expect("pdf generation failed");
        assert_eq!(&pdf[..4], b"%PDF");
        assert!(pdf.len() > 2000);
    }

    #[test]
    fn test_generate_invoice_pdf_minimal() {
        let mut data = sample_invoice();
        data.billing = None;
        data.shipping = None;
        data.shipping_method = None;
        data.items.clear();
        let pdf = generate_invoice_pdf(&data).expect("pdf generation failed");
        assert!(pdf.len() > 2000);
        assert_eq!(&pdf[..4], b"%PDF");
    }

    #[test]
    fn test_format_address() {
        let addr = InvoiceAddress {
            first_name: "Jane".to_string(),
            last_name: "Smith".to_string(),
            address_1: "1 Road".to_string(),
            address_2: Some("Floor 2".to_string()),
            city: "Hong Kong".to_string(),
            state: None,
            postcode: "0000".to_string(),
            country: "Hong Kong".to_string(),
            phone: None,
        };
        let lines = format_address(&addr);
        assert_eq!(lines[0], "Jane Smith");
        assert_eq!(lines[1], "Floor 2");
        assert_eq!(lines[2], "1 Road");
        assert_eq!(lines[3], "Hong Kong 0000");
    }

    #[test]
    fn test_wrap_text() {
        let long = "A very long product name that will definitely need to wrap onto multiple lines"
            .to_string();
        let lines = wrap_text(&long, 9.5, 200.0);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(estimate_width(line, 9.5, false) <= 200.0 + 20.0);
        }
    }

    #[test]
    fn test_currency_symbol() {
        assert_eq!(currency_symbol("USD"), "$");
        assert_eq!(currency_symbol("EUR"), "€");
        assert_eq!(currency_symbol("GBP"), "£");
        assert_eq!(currency_symbol("JPY"), "¥");
        assert_eq!(currency_symbol("HKD"), "HK$");
        assert_eq!(currency_symbol("CHF"), "CHF");
    }

    #[test]
    fn test_display_shipping_method() {
        assert_eq!(display_shipping_method("standard"), "Standard");
        assert_eq!(
            display_shipping_method("standard_us_shipping"),
            "Standard US Shipping"
        );
        assert_eq!(display_shipping_method("Express"), "Express");
    }
}
