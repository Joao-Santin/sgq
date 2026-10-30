//! "Produções" tab: OPs from the ERP and the quality PDFs attached to each product.

use iced::widget::{button, checkbox, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length};

use crate::erp::{Production, ProductionItem, Situation};
use crate::quality::{DocId, Unit};
use crate::widgets::{action_button, action_button_maybe, br_date, hint, panel, quantity, tab_button};
use crate::{App, Message, Pending, Remote, permissions};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SituationFilter {
    #[default]
    All,
    Open,
    Done,
}

impl SituationFilter {
    const ALL: [SituationFilter; 3] = [SituationFilter::All, SituationFilter::Open, SituationFilter::Done];

    fn label(self) -> &'static str {
        match self {
            SituationFilter::All => "Todas",
            SituationFilter::Open => "Em aberto",
            SituationFilter::Done => "Concluídas",
        }
    }

    fn matches(self, situation: Option<Situation>) -> bool {
        match self {
            SituationFilter::All => true,
            SituationFilter::Open => situation == Some(Situation::Open),
            SituationFilter::Done => situation == Some(Situation::Done),
        }
    }
}

impl App {
    pub(crate) fn productions_tab(&self) -> Element<'_, Message> {
        let loading = self.productions.is_loading();

        let toolbar = row![
            text("Início de").size(14),
            text_input("DD/MM/AAAA", &self.period_from)
                .on_input(Message::PeriodFromChanged)
                .on_submit(Message::ProductionsRefresh)
                .padding(8)
                .width(Length::Fixed(120.0)),
            text("até").size(14),
            text_input("DD/MM/AAAA", &self.period_to)
                .on_input(Message::PeriodToChanged)
                .on_submit(Message::ProductionsRefresh)
                .padding(8)
                .width(Length::Fixed(120.0)),
            action_button_maybe(
                if loading { "Buscando..." } else { "Buscar no ERP" },
                (!loading).then_some(Message::ProductionsRefresh),
                button::primary,
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let situation = row(SituationFilter::ALL.into_iter().map(|filter| {
            tab_button(
                filter.label(),
                Message::SituationFilterChanged(filter),
                filter == self.situation_filter,
            )
        }))
        .spacing(6);

        let filters = row![
            text("Situação:").size(14),
            situation,
            space().width(12),
            checkbox(self.only_pending)
                .label("Somente com pendências")
                .on_toggle(Message::OnlyPendingToggled)
                .size(16)
                .text_size(14),
            space::horizontal(),
            text_input("Buscar OP ou produto", &self.production_search)
                .on_input(Message::ProductionSearchChanged)
                .padding(8)
                .width(Length::Fixed(240.0)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let productions = match &self.productions {
            Remote::Idle => return column![toolbar, filters].spacing(14).into(),
            Remote::Loading => {
                return column![toolbar, filters, hint("Buscando produções no e-Gestor...")]
                    .spacing(14)
                    .into();
            }
            Remote::Failed(e) => {
                return column![toolbar, filters, text(e.clone()).size(14).style(text::danger)]
                    .spacing(14)
                    .into();
            }
            Remote::Loaded(list) => list,
        };

        let search = self.production_search.trim().to_lowercase();
        let visible: Vec<&Production> = productions
            .iter()
            .filter(|op| self.situation_filter.matches(op.situation))
            .filter(|op| !self.only_pending || self.op_pending(op) > 0)
            .filter(|op| {
                search.is_empty()
                    || op.code.to_string().contains(&search)
                    || op.items.iter().any(|item| {
                        item.description.to_lowercase().contains(&search)
                            || item.own_code.to_lowercase().contains(&search)
                    })
            })
            .collect();

        let shown = visible.len();
        let with_pending = productions.iter().filter(|op| self.op_pending(op) > 0).count();
        let summary = hint(format!(
            "{} OPs no período · {with_pending} com documentos aguardando anexo · {shown} exibidas",
            productions.len()
        ));

        let list: Element<'_, Message> = if visible.is_empty() {
            hint("Nenhuma OP encontrada.")
        } else {
            column(visible.into_iter().map(|op| self.op_card(op))).spacing(6).into()
        };

        let detail = self
            .selected_op
            .and_then(|code| productions.iter().find(|op| op.code == code))
            .map_or_else(|| panel(hint("Selecione uma OP na lista.")), |op| self.op_detail(op));

        column![
            toolbar,
            filters,
            summary,
            row![
                scrollable(list).width(Length::Fixed(380.0)).height(Length::Fill),
                scrollable(detail).width(Length::Fill).height(Length::Fill),
            ]
            .spacing(20)
            .height(Length::Fill),
        ]
        .spacing(14)
        .into()
    }

    /// Documentation units of an OP: one per product, or one per group when
    /// two or more products of the same group are in the OP.
    fn op_units(&self, op: &Production) -> Vec<Unit> {
        self.quality.units(op.items.iter().map(|item| item.product))
    }

    /// Documents missing across every unit of the OP.
    fn op_pending(&self, op: &Production) -> usize {
        self.op_units(op)
            .iter()
            .map(|unit| self.quality.pending(op.code, unit))
            .sum()
    }

    fn op_card<'a>(&'a self, op: &'a Production) -> Element<'a, Message> {
        let units = self.op_units(op);
        let pending: usize = units.iter().map(|unit| self.quality.pending(op.code, unit)).sum();
        let required: usize = units.iter().map(|unit| self.quality.unit_required(unit).len()).sum();

        let status = if required == 0 {
            text("Nenhum documento exigido").size(12).style(text::secondary)
        } else if pending > 0 {
            text(pending_label(pending)).size(12).style(text::danger)
        } else {
            text("Documentação completa").size(12).style(text::success)
        };

        let products = match op.items.as_slice() {
            [single] => single.description.clone(),
            items => format!("{} produtos", items.len()),
        };

        let content = column![
            row![
                text(format!("OP {}", op.code)).size(15),
                space::horizontal(),
                text(op.situation.map_or("", |s| s.label())).size(12).style(text::secondary),
            ],
            text(products).size(13),
            row![
                text(format!("Início {}", br_date(&op.start))).size(12).style(text::secondary),
                space::horizontal(),
                status,
            ],
        ]
        .spacing(4);

        button(content)
            .on_press(Message::OpSelected(op.code))
            .width(Length::Fill)
            .padding([8, 12])
            .style(if self.selected_op == Some(op.code) { button::secondary } else { button::text })
            .into()
    }

    fn op_detail<'a>(&'a self, op: &'a Production) -> Element<'a, Message> {
        let header = column![
            text(format!("OP {}", op.code)).size(22),
            hint(format!(
                "{} · início {} · {} produto(s)",
                op.situation.map_or("Situação desconhecida", |s| s.label()),
                br_date(&op.start),
                op.items.len()
            )),
        ]
        .spacing(4);

        let header = row![header, space::horizontal()]
            .push(self.group_from_op_button(op))
            .align_y(Alignment::Center);

        column![header]
            .extend(self.op_units(op).into_iter().map(|unit| self.unit_panel(op, unit)))
            .spacing(12)
            .padding([0, 12])
            .into()
    }

    /// "Create a group with every product of this OP", for users who manage documents.
    fn group_from_op_button(&self, op: &Production) -> Option<Element<'_, Message>> {
        if !self.can(permissions::DOCUMENTS_CONFIGURE) {
            return None;
        }

        let mut products: Vec<i64> = op.items.iter().map(|item| item.product).collect();
        products.sort_unstable();
        products.dedup();
        if products.len() < 2 {
            return None;
        }

        let first_group = self.quality.group_of(products[0]).map(|g| g.id);
        let already_grouped = first_group.is_some()
            && products
                .iter()
                .all(|p| self.quality.group_of(*p).map(|g| g.id) == first_group);

        Some(action_button_maybe(
            if already_grouped { "Itens já estão no mesmo grupo" } else { "Criar grupo com os itens desta OP" },
            (!already_grouped)
                .then_some(Message::Request(Pending::CreateGroup { op: Some(op.code), products })),
            button::secondary,
        ))
    }

    fn unit_panel<'a>(&'a self, op: &'a Production, unit: Unit) -> Element<'a, Message> {
        let pending = self.quality.pending(op.code, &unit);
        let docs = self.quality.docs_for(op.code, &unit);

        let status: Element<'_, Message> = if docs.is_empty() {
            hint("Nenhum documento exigido.")
        } else if pending > 0 {
            text(pending_label(pending)).size(13).style(text::danger).into()
        } else {
            text("Documentação completa").size(13).style(text::success).into()
        };

        let items: Vec<&ProductionItem> = op
            .items
            .iter()
            .filter(|item| unit.products.contains(&item.product))
            .collect();

        let mut content = column![].spacing(8);
        if let Some(group) = unit.group.and_then(|id| self.quality.group(id)) {
            content = content
                .push(text(format!("Grupo: {}", group.name)).size(16))
                .push(hint("Os documentos abaixo valem para todos os produtos do grupo nesta OP."));
            for item in &items {
                content = content.push(
                    text(format!(
                        "{} · {} — {} {}",
                        item.own_code,
                        item.description,
                        quantity(item.quantity),
                        item.unit
                    ))
                    .size(13),
                );
            }
        } else if let Some(item) = items.first() {
            content = content
                .push(text(format!("{} · {}", item.own_code, item.description)).size(16))
                .push(hint(format!("{} {}", quantity(item.quantity), item.unit)));
        }

        let required = self.quality.unit_required(&unit);
        let rows: Vec<Element<'_, Message>> = docs
            .into_iter()
            .map(|doc| self.doc_row(op.code, &unit, doc, required.contains(&doc)))
            .collect();

        panel(content.push(status).extend(rows))
    }

    fn doc_row(&self, op: i64, unit: &Unit, doc: DocId, required: bool) -> Element<'_, Message> {
        let Some(doc_type) = self.quality.doc_type(doc) else {
            return space().into();
        };
        let versions = self.quality.versions(op, unit, doc);

        let status = match versions.first() {
            Some(latest) => text(format!("Rev. {:02} anexada", latest.version)).size(13).style(text::success),
            None => text("Aguardando anexo").size(13).style(text::danger),
        };

        let attach = self
            .can(permissions::PRODUCTIONS_ATTACH)
            .then(|| Message::Attach { op, unit: unit.clone(), doc });

        let title = if required {
            format!("{} — {}", doc_type.code, doc_type.name)
        } else {
            format!("{} — {} (não exigido)", doc_type.code, doc_type.name)
        };

        let heading = row![
            text(title).size(14),
            space::horizontal(),
            status,
            action_button_maybe(
                if versions.is_empty() { "Anexar PDF" } else { "Nova versão" },
                attach,
                button::primary,
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let history = versions.iter().enumerate().map(|(i, attachment)| {
            let current = if i == 0 { " (atual)" } else { "" };
            row![
                text(format!(
                    "Rev. {:02}{current} · {} · {} em {}",
                    attachment.version,
                    attachment.original_name,
                    attachment.uploaded_by,
                    attachment.uploaded_at
                ))
                .size(12)
                .style(text::secondary),
                space::horizontal(),
                action_button(
                    "Abrir",
                    Message::OpenFile(self.quality.file_path(attachment)),
                    button::secondary,
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        });

        container(column![heading].extend(history).spacing(6))
            .padding([8, 10])
            .style(container::bordered_box)
            .width(Length::Fill)
            .into()
    }
}

fn pending_label(count: usize) -> String {
    if count == 1 {
        "1 documento aguardando anexo".into()
    } else {
        format!("{count} documentos aguardando anexo")
    }
}
