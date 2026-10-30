//! Administrator tabs: user approvals and permissions.

use iced::widget::{Row, button, checkbox, column, container, pick_list, row, scrollable, space, text};
use iced::{Alignment, Element, Length};

use crate::auth::{Status, User};
use crate::widgets::{action_button, hint, panel};
use crate::{App, Message, permissions};

impl App {
    pub(crate) fn users_tab(&self) -> Element<'_, Message> {
        let pending = user_section(
            "Aguardando aprovação",
            self.store.with_status(Status::Pending),
            |name| {
                row![
                    action_button("Aprovar", Message::SetStatus(name.clone(), Status::Approved), button::success),
                    action_button("Recusar", Message::SetStatus(name, Status::Rejected), button::danger),
                ]
                .spacing(8)
            },
        );

        let approved = user_section(
            "Usuários aprovados",
            self.store.with_status(Status::Approved),
            |name| row![action_button("Remover", Message::Remove(name), button::danger)],
        );

        let rejected = user_section(
            "Cadastros recusados",
            self.store.with_status(Status::Rejected),
            |name| {
                row![
                    action_button("Aprovar", Message::SetStatus(name.clone(), Status::Approved), button::success),
                    action_button("Remover", Message::Remove(name), button::secondary),
                ]
                .spacing(8)
            },
        );

        scrollable(column![pending, approved, rejected].spacing(24))
            .height(Length::Fill)
            .into()
    }

    pub(crate) fn permissions_tab(&self) -> Element<'_, Message> {
        let approved: Vec<String> = self
            .store
            .with_status(Status::Approved)
            .map(|u| u.username.clone())
            .collect();

        if approved.is_empty() {
            return hint("Nenhum usuário aprovado. Aprove um cadastro na aba Usuários.");
        }

        let picker = row![
            text("Usuário:").size(14),
            pick_list(approved, self.selected_user.clone(), Message::UserSelected)
                .placeholder("Selecione um usuário")
                .width(Length::Fixed(320.0)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let Some(user) = self.selected_user.as_deref().and_then(|u| self.store.get(u)) else {
            return column![
                picker,
                hint("Selecione um usuário para editar o que ele pode acessar e fazer."),
            ]
            .spacing(16)
            .into();
        };

        let bulk = row![
            action_button(
                "Conceder todas",
                Message::SetAllPermissions(user.username.clone(), true),
                button::success,
            ),
            action_button(
                "Revogar todas",
                Message::SetAllPermissions(user.username.clone(), false),
                button::danger,
            ),
        ]
        .spacing(8);

        let sections = permissions::SECTIONS.iter().map(|section| {
            let toggles = section.permissions.iter().map(|permission| {
                let username = user.username.clone();
                checkbox(user.can(permission.key))
                    .label(permission.label)
                    .on_toggle(move |granted| {
                        Message::PermissionToggled(username.clone(), permission.key, granted)
                    })
                    .size(16)
                    .text_size(14)
                    .into()
            });

            panel(column![text(section.name).size(18), column(toggles).spacing(10)].spacing(12))
        });

        scrollable(
            column![picker, bulk, column(sections).spacing(12)]
                .spacing(16)
                .max_width(720),
        )
        .height(Length::Fill)
        .into()
    }
}

fn user_section<'a>(
    title: &'a str,
    users: impl Iterator<Item = &'a User>,
    actions: impl Fn(String) -> Row<'a, Message>,
) -> Element<'a, Message> {
    let rows: Vec<Element<'a, Message>> = users
        .map(|user| {
            container(
                row![
                    text(&user.username).size(16),
                    space::horizontal(),
                    actions(user.username.clone()),
                ]
                .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .style(container::rounded_box)
            .into()
        })
        .collect();

    let count = rows.len();
    let body: Element<'a, Message> = if rows.is_empty() {
        hint("Nenhum usuário.")
    } else {
        column(rows).spacing(8).into()
    };

    column![text(format!("{title} ({count})")).size(18), body]
        .spacing(10)
        .max_width(720)
        .into()
}
