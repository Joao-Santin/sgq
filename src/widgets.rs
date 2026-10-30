//! Small reusable widgets shared by every screen.

use iced::widget::{Column, button, center, column, container, opaque, space, stack, text};
use iced::{Alignment, Color, Element, Length, Theme};

use crate::Message;

pub type ButtonStyle = fn(&Theme, button::Status) -> button::Style;

pub fn header<'a>(title: &'a str, subtitle: &'a str) -> Element<'a, Message> {
    column![
        text(title).size(28),
        text(subtitle).size(14).style(text::secondary).center(),
        space().height(12),
    ]
    .spacing(4)
    .align_x(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// Width of the login/registration form, excluding the card padding.
const CARD_WIDTH: f32 = 360.0;
const CARD_PADDING: f32 = 32.0;

pub fn card<'a>(form: Column<'a, Message>) -> Element<'a, Message> {
    // Fixed width: the form's children fill horizontally, which would
    // otherwise stretch the card across the whole window.
    let card = container(form.spacing(16))
        .padding(CARD_PADDING)
        .width(Length::Fixed(CARD_WIDTH + 2.0 * CARD_PADDING))
        .style(container::rounded_box);

    container(card).center(Length::Fill).padding(16).into()
}

/// Shows `dialog` centered over `base`, dimming it and blocking its input.
pub fn modal<'a>(
    base: impl Into<Element<'a, Message>>,
    dialog: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    stack![
        base.into(),
        opaque(center(opaque(dialog)).style(|_theme| container::Style {
            background: Some(Color { a: 0.7, ..Color::BLACK }.into()),
            ..container::Style::default()
        })),
    ]
    .into()
}

pub fn primary_button(label: &str, message: Message) -> Element<'_, Message> {
    button(text(label).width(Length::Fill).center())
        .on_press(message)
        .padding(12)
        .width(Length::Fill)
        .style(button::primary)
        .into()
}

pub fn link_button(label: &str, message: Message) -> Element<'_, Message> {
    container(button(text(label).size(14)).on_press(message).style(button::text))
        .center_x(Length::Fill)
        .into()
}

pub fn action_button(label: &str, message: Message, style: ButtonStyle) -> Element<'_, Message> {
    action_button_maybe(label, Some(message), style)
}

/// Disabled (greyed out) when `message` is `None`.
pub fn action_button_maybe(
    label: &str,
    message: Option<Message>,
    style: ButtonStyle,
) -> Element<'_, Message> {
    button(text(label).size(14))
        .on_press_maybe(message)
        .padding([6, 14])
        .style(style)
        .into()
}

pub fn tab_button(label: &str, message: Message, active: bool) -> Element<'_, Message> {
    button(text(label).size(14))
        .on_press(message)
        .padding([8, 16])
        .style(if active { button::primary } else { button::secondary })
        .into()
}

pub fn hint(content: impl text::IntoFragment<'static>) -> Element<'static, Message> {
    text(content).size(14).style(text::secondary).into()
}

/// Section title with a rounded box around its content.
pub fn panel<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding([12, 16])
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

/// Converts a YYYY-MM-DD date (as the ERP sends) to DD/MM/YYYY.
pub fn br_date(iso: &str) -> String {
    let date = iso.get(..10).unwrap_or(iso);
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|_| iso.to_owned())
}

/// Parses a DD/MM/YYYY date typed by the user into YYYY-MM-DD.
pub fn iso_date(br: &str) -> Option<String> {
    chrono::NaiveDate::parse_from_str(br.trim(), "%d/%m/%Y")
        .ok()
        .map(|d| d.format("%Y-%m-%d").to_string())
}

pub fn quantity(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.3}").trim_end_matches('0').replace('.', ",")
    }
}
