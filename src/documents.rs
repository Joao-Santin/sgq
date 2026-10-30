//! Tabs that configure which quality documents exist and which products require them.

use iced::widget::{button, checkbox, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length};

use crate::widgets::{action_button, action_button_maybe, hint, panel, tab_button};
use crate::{App, Message, Pending, Remote, SpecificTab};

/// Keeps the product list responsive; the search narrows it down.
const MAX_PRODUCTS_SHOWN: usize = 150;
const MAX_GROUP_RESULTS: usize = 30;

impl App {
    pub(crate) fn doc_types_tab(&self) -> Element<'_, Message> {
        let form = row![
            text_input("Código (ex.: FCQ-01)", &self.new_doc_code)
                .on_input(Message::DocCodeChanged)
                .on_submit(Message::AddDocType)
                .padding(8)
                .width(Length::Fixed(180.0)),
            text_input("Nome do documento", &self.new_doc_name)
                .on_input(Message::DocNameChanged)
                .on_submit(Message::AddDocType)
                .padding(8)
                .width(Length::Fill),
            action_button("Adicionar", Message::AddDocType, button::primary),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let types = self.quality.doc_types();
        let list: Element<'_, Message> = if types.is_empty() {
            hint("Nenhum tipo de documento cadastrado ainda.")
        } else {
            column(types.iter().map(|doc| {
                panel(
                    row![
                        text(&doc.code).size(15).width(Length::Fixed(140.0)),
                        text(&doc.name).size(15),
                        space::horizontal(),
                        action_button("Remover", Message::Request(Pending::DeleteDocType(doc.id)), button::danger),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
            }))
            .spacing(8)
            .into()
        };

        scrollable(
            column![
                text("Tipos de documento").size(20),
                hint("Cadastre todos os documentos (PDFs) que podem ser exigidos nas OPs."),
                form,
                list,
            ]
            .spacing(14)
            .max_width(820),
        )
        .height(Length::Fill)
        .into()
    }

    pub(crate) fn general_needs_tab(&self) -> Element<'_, Message> {
        let types = self.quality.doc_types();

        let list: Element<'_, Message> = if types.is_empty() {
            hint("Cadastre os documentos na aba Tipos de documento primeiro.")
        } else {
            panel(
                column(types.iter().map(|doc| {
                    let id = doc.id;
                    checkbox(self.quality.is_general(id))
                        .label(format!("{} — {}", doc.code, doc.name))
                        .on_toggle(move |required| Message::GeneralToggled(id, required))
                        .size(16)
                        .text_size(14)
                        .into()
                }))
                .spacing(10),
            )
        };

        scrollable(
            column![
                text("Necessidades gerais").size(20),
                hint("Documentos exigidos de todos os produtos, em todas as OPs."),
                list,
            ]
            .spacing(14)
            .max_width(820),
        )
        .height(Length::Fill)
        .into()
    }

    pub(crate) fn specific_needs_tab(&self) -> Element<'_, Message> {
        let tabs = row![
            tab_button(
                "Documentos",
                Message::SpecificTabSelected(SpecificTab::Documents),
                self.specific_tab == SpecificTab::Documents,
            ),
            tab_button(
                "Grupos",
                Message::SpecificTabSelected(SpecificTab::Groups),
                self.specific_tab == SpecificTab::Groups,
            ),
        ]
        .spacing(6);

        let body = match self.specific_tab {
            SpecificTab::Documents => self.specific_documents(),
            SpecificTab::Groups => self.groups_view(),
        };

        column![
            text("Necessidades específicas").size(20),
            tabs,
            body,
        ]
        .spacing(10)
        .into()
    }

    fn specific_documents(&self) -> Element<'_, Message> {
        let loading = self.products_status.is_loading();

        let toolbar = row![
            text_input("Buscar produto por código ou descrição", &self.product_search)
                .on_input(Message::ProductSearchChanged)
                .padding(8)
                .width(Length::Fill),
            action_button_maybe(
                if loading { "Buscando..." } else { "Atualizar do ERP" },
                (!loading).then_some(Message::ProductsRefresh),
                button::secondary,
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let status: Element<'_, Message> = match &self.products_status {
            Remote::Loading => hint("Buscando todos os produtos no e-Gestor. Isso pode levar alguns segundos..."),
            Remote::Failed(e) => text(e.clone()).size(14).style(text::danger).into(),
            Remote::Idle | Remote::Loaded(()) => {
                hint(format!("{} produtos cadastrados no ERP.", self.products.len()))
            }
        };

        let search = self.product_search.trim().to_lowercase();
        let mut matches = self.products.iter().filter(|p| {
            search.is_empty()
                || p.descricao.to_lowercase().contains(&search)
                || p.codigo_proprio.to_lowercase().contains(&search)
        });
        let shown: Vec<_> = matches.by_ref().take(MAX_PRODUCTS_SHOWN).collect();
        let hidden = matches.count();

        let mut list = column(shown.into_iter().map(|product| {
            let specific = self.quality.specific_count(product.codigo);
            let selected = self.selected_product == Some(product.codigo);

            button(
                row![
                    text(&product.codigo_proprio).size(13).width(Length::Fixed(80.0)),
                    text(&product.descricao).size(13).width(Length::Fill),
                    text(if specific > 0 { format!("{specific} específico(s)") } else { String::new() })
                        .size(12)
                        .style(text::primary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ProductSelected(product.codigo))
            .width(Length::Fill)
            .padding([6, 10])
            .style(if selected { button::secondary } else { button::text })
            .into()
        }))
        .spacing(2);

        if hidden > 0 {
            list = list.push(hint(format!("+ {hidden} produtos. Refine a busca para vê-los.")));
        }

        let left = column![toolbar, status, scrollable(list).height(Length::Fill)]
            .spacing(10)
            .width(Length::FillPortion(3));

        let right = container(self.product_requirements())
            .width(Length::FillPortion(2))
            .height(Length::Fill);

        column![
            hint("Documentos exigidos além dos gerais, só para os produtos escolhidos."),
            row![left, right].spacing(20).height(Length::Fill),
        ]
        .spacing(10)
        .into()
    }

    fn product_requirements(&self) -> Element<'_, Message> {
        let Some(product) = self
            .selected_product
            .and_then(|code| self.products.iter().find(|p| p.codigo == code))
        else {
            return panel(hint("Selecione um produto na lista."));
        };

        let types = self.quality.doc_types();
        let toggles: Element<'_, Message> = if types.is_empty() {
            hint("Cadastre os documentos na aba Tipos de documento primeiro.")
        } else {
            column(types.iter().map(|doc| {
                let (id, code) = (doc.id, product.codigo);
                if self.quality.is_general(id) {
                    // Already required for every product; shown for reference only.
                    checkbox(true)
                        .label(format!("{} — {} (geral)", doc.code, doc.name))
                        .size(16)
                        .text_size(14)
                        .into()
                } else {
                    checkbox(self.quality.is_specific(code, id))
                        .label(format!("{} — {}", doc.code, doc.name))
                        .on_toggle(move |required| Message::SpecificToggled(code, id, required))
                        .size(16)
                        .text_size(14)
                        .into()
                }
            }))
            .spacing(10)
            .into()
        };

        let group = self.quality.group_of(product.codigo).map(|group| {
            text(format!(
                "Grupo: {} — compartilha os documentos quando dois ou mais itens do grupo estão na mesma OP.",
                group.name
            ))
            .size(13)
            .style(text::primary)
        });

        scrollable(panel(
            column![
                text(&product.descricao).size(16),
                hint(format!("Código: {}", product.codigo_proprio)),
            ]
            .push(group)
            .push(
                text(format!(
                    "{} documento(s) exigido(s) por OP",
                    self.quality.required(product.codigo).len()
                ))
                .size(14),
            )
            .push(toggles)
            .spacing(12),
        ))
        .into()
    }

    fn groups_view(&self) -> Element<'_, Message> {
        let name = self.new_group_name.trim();
        let create = row![
            text_input("Nome do novo grupo", &self.new_group_name)
                .on_input(Message::NewGroupNameChanged)
                .on_submit_maybe((!name.is_empty()).then(create_group_request))
                .padding(8)
                .width(Length::Fill),
            action_button_maybe(
                "Criar grupo",
                (!name.is_empty()).then(create_group_request),
                button::primary,
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let groups = self.quality.groups();
        let list: Element<'_, Message> = if groups.is_empty() {
            hint("Nenhum grupo criado ainda.")
        } else {
            column(groups.iter().map(|group| {
                button(
                    row![
                        text(&group.name).size(14).width(Length::Fill),
                        text(format!("{} produto(s)", group.products.len()))
                            .size(12)
                            .style(text::secondary),
                    ]
                    .spacing(8),
                )
                .on_press(Message::GroupSelected(group.id))
                .width(Length::Fill)
                .padding([6, 10])
                .style(if self.selected_group == Some(group.id) {
                    button::secondary
                } else {
                    button::text
                })
                .into()
            }))
            .spacing(2)
            .into()
        };

        let left = column![create, scrollable(list).height(Length::Fill)]
            .spacing(10)
            .width(Length::FillPortion(2));

        let right = container(self.group_detail())
            .width(Length::FillPortion(3))
            .height(Length::Fill);

        column![
            hint(
                "Produtos que compartilham os mesmos documentos (ex.: o mesmo item em cores diferentes). \
                 Quando dois ou mais produtos do grupo aparecem na mesma OP, cada documento é exigido uma única vez."
            ),
            row![left, right].spacing(20).height(Length::Fill),
        ]
        .spacing(10)
        .into()
    }

    fn group_detail(&self) -> Element<'_, Message> {
        let Some(group) = self.selected_group.and_then(|id| self.quality.group(id)) else {
            return panel(hint("Selecione um grupo na lista."));
        };

        let product_label = |code: i64| {
            self.products.iter().find(|p| p.codigo == code).map_or_else(
                || format!("Produto {code}"),
                |p| format!("{} · {}", p.codigo_proprio, p.descricao),
            )
        };

        let members: Element<'_, Message> = if group.products.is_empty() {
            hint("Nenhum produto no grupo. Adicione produtos pela busca abaixo.")
        } else {
            column(group.products.iter().map(|&code| {
                row![
                    text(product_label(code)).size(13).width(Length::Fill),
                    action_button(
                        "Remover",
                        Message::Request(Pending::RemoveFromGroup(group.id, code)),
                        button::secondary,
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
            }))
            .spacing(6)
            .into()
        };

        let search = self.group_search.trim().to_lowercase();
        let results: Element<'_, Message> = if search.is_empty() {
            hint("Digite para buscar produtos que ainda não estão em nenhum grupo.")
        } else {
            let found: Vec<_> = self
                .products
                .iter()
                .filter(|p| self.quality.group_of(p.codigo).is_none())
                .filter(|p| {
                    p.descricao.to_lowercase().contains(&search)
                        || p.codigo_proprio.to_lowercase().contains(&search)
                })
                .take(MAX_GROUP_RESULTS)
                .collect();

            if found.is_empty() {
                hint("Nenhum produto disponível encontrado.")
            } else {
                column(found.into_iter().map(|p| {
                    row![
                        text(format!("{} · {}", p.codigo_proprio, p.descricao))
                            .size(13)
                            .width(Length::Fill),
                        action_button(
                            "Adicionar",
                            Message::AddToGroup(group.id, p.codigo),
                            button::success,
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into()
                }))
                .spacing(6)
                .into()
            }
        };

        scrollable(panel(
            column![
                row![
                    text(&group.name).size(18).width(Length::Fill),
                    action_button(
                        "Excluir grupo",
                        Message::Request(Pending::DeleteGroup(group.id)),
                        button::danger,
                    ),
                ]
                .align_y(Alignment::Center),
                text(format!("Produtos ({})", group.products.len())).size(14),
                members,
                text("Adicionar produtos").size(14),
                text_input("Buscar produto por código ou descrição", &self.group_search)
                    .on_input(Message::GroupSearchChanged)
                    .padding(8),
                results,
            ]
            .spacing(12),
        ))
        .into()
    }
}

fn create_group_request() -> Message {
    Message::Request(Pending::CreateGroup { op: None, products: Vec::new() })
}
