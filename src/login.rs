use iced::Element;
use iced::widget::{checkbox, column, text, text_input};

use crate::widgets::{card, header, link_button, primary_button};
use crate::{App, CONFIRM_ID, Message, PASSWORD_ID, USERNAME_ID};

impl App {
    pub(crate) fn login_view(&self) -> Element<'_, Message> {
        let form = column![
            header("Bem-vindo", "Entre para continuar"),
            self.username_field(Message::FocusNext(PASSWORD_ID)),
            self.password_field(PASSWORD_ID, "Senha", &self.password, Message::PasswordChanged, Message::Submit),
            self.show_password_toggle(),
        ]
        .push(self.notice_view())
        .push(primary_button("Entrar", Message::Submit))
        .push(link_button("Não tem conta? Cadastre-se", Message::GoToRegister));

        card(form)
    }

    pub(crate) fn register_view(&self) -> Element<'_, Message> {
        let form = column![
            header("Criar conta", "O cadastro precisa ser aprovado pelo administrador"),
            self.username_field(Message::FocusNext(PASSWORD_ID)),
            self.password_field(PASSWORD_ID, "Senha", &self.password, Message::PasswordChanged, Message::FocusNext(CONFIRM_ID)),
            self.password_field(CONFIRM_ID, "Confirmar senha", &self.confirm, Message::ConfirmChanged, Message::Register),
            self.show_password_toggle(),
        ]
        .push(self.notice_view())
        .push(primary_button("Cadastrar", Message::Register))
        .push(link_button("Já tem conta? Entrar", Message::GoToLogin));

        card(form)
    }

    fn username_field(&self, on_submit: Message) -> Element<'_, Message> {
        column![
            text("Usuário").size(14),
            text_input("Digite seu usuário", &self.username)
                .id(USERNAME_ID)
                .on_input(Message::UsernameChanged)
                .on_submit(on_submit)
                .padding(10),
        ]
        .spacing(6)
        .into()
    }

    fn password_field<'a>(
        &self,
        id: &'static str,
        label: &'a str,
        value: &'a str,
        on_input: fn(String) -> Message,
        on_submit: Message,
    ) -> Element<'a, Message> {
        column![
            text(label).size(14),
            text_input("Digite sua senha", value)
                .id(id)
                .secure(!self.show_password)
                .on_input(on_input)
                .on_submit(on_submit)
                .padding(10),
        ]
        .spacing(6)
        .into()
    }

    fn show_password_toggle(&self) -> Element<'_, Message> {
        checkbox(self.show_password)
            .label("Mostrar senha")
            .on_toggle(Message::ShowPasswordToggled)
            .size(16)
            .text_size(14)
            .into()
    }
}
