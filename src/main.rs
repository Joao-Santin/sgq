mod admin;
mod auth;
mod documents;
mod erp;
mod login;
mod permissions;
mod productions;
mod quality;
mod widgets;

use std::path::PathBuf;

use auth::{LoginResult, Status, UserStore};
use erp::{Product, Production};
use iced::widget::{column, container, operation, row, rule, space, text};
use iced::{Alignment, Element, Length, Task, Theme};
use quality::{DocId, GroupId, QualityStore, Unit};
use widgets::{action_button, tab_button};

const USERNAME_ID: &str = "username";
const PASSWORD_ID: &str = "password";
const CONFIRM_ID: &str = "confirm";
const DIALOG_NAME_ID: &str = "dialog_name";
const DIALOG_USERNAME_ID: &str = "dialog_username";
const DIALOG_PASSWORD_ID: &str = "dialog_password";

/// How many days back the productions tab looks by default.
const DEFAULT_PERIOD_DAYS: i64 = 30;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("SGQ")
        .theme(App::theme)
        .window_size((1200.0, 760.0))
        .centered()
        .run()
}

#[derive(Default)]
enum Screen {
    #[default]
    Login,
    Register,
    Main,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Home,
    Productions,
    DocTypes,
    GeneralNeeds,
    SpecificNeeds,
    Users,
    Permissions,
}

impl Tab {
    const ALL: [Tab; 7] = [
        Tab::Home,
        Tab::Productions,
        Tab::DocTypes,
        Tab::GeneralNeeds,
        Tab::SpecificNeeds,
        Tab::Users,
        Tab::Permissions,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Home => "Início",
            Tab::Productions => "Produções",
            Tab::DocTypes => "Tipos de documento",
            Tab::GeneralNeeds => "Necessidades gerais",
            Tab::SpecificNeeds => "Necessidades específicas",
            Tab::Users => "Usuários",
            Tab::Permissions => "Autorizações",
        }
    }

    /// Permission needed to see the tab; `None` means administrator only.
    fn permission(self) -> Option<&'static str> {
        match self {
            Tab::Home => Some(permissions::HOME_ACCESS),
            Tab::Productions => Some(permissions::PRODUCTIONS_VIEW),
            Tab::DocTypes | Tab::GeneralNeeds | Tab::SpecificNeeds => {
                Some(permissions::DOCUMENTS_CONFIGURE)
            }
            Tab::Users | Tab::Permissions => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SpecificTab {
    #[default]
    Documents,
    Groups,
}

struct Session {
    username: String,
    is_admin: bool,
}

/// An action waiting for confirmation in a pop-up. Deletions also require the
/// current user to type their username and password again.
#[derive(Debug, Clone)]
enum Pending {
    /// `op` is set when the group is created from the products of an OP.
    CreateGroup { op: Option<i64>, products: Vec<i64> },
    DeleteGroup(GroupId),
    RemoveFromGroup(GroupId, i64),
    DeleteDocType(DocId),
    RemoveSpecific(i64, DocId),
}

impl Pending {
    fn is_deletion(&self) -> bool {
        !matches!(self, Pending::CreateGroup { .. })
    }
}

struct Dialog {
    action: Pending,
    title: String,
    message: String,
    /// Group name, editable when creating a group.
    name: String,
    username: String,
    password: String,
    error: Option<String>,
}

enum Notice {
    Error(String),
    Info(String),
}

/// Data loaded from the ERP in the background.
enum Remote<T> {
    Idle,
    Loading,
    Failed(String),
    Loaded(T),
}

impl<T> Remote<T> {
    fn is_loading(&self) -> bool {
        matches!(self, Remote::Loading)
    }
}

struct App {
    store: UserStore,
    quality: QualityStore,
    screen: Screen,
    session: Option<Session>,
    tab: Tab,
    notice: Option<Notice>,
    dialog: Option<Dialog>,

    // Login / registration form.
    username: String,
    password: String,
    confirm: String,
    show_password: bool,

    // "Autorizações" tab.
    selected_user: Option<String>,

    // ERP products ("Necessidades específicas" tab).
    products: Vec<Product>,
    products_status: Remote<()>,
    product_search: String,
    selected_product: Option<i64>,
    specific_tab: SpecificTab,

    // Groups ("Necessidades específicas" → "Grupos").
    selected_group: Option<GroupId>,
    new_group_name: String,
    group_search: String,

    // ERP productions ("Produções" tab).
    productions: Remote<Vec<Production>>,
    period_from: String,
    period_to: String,
    production_search: String,
    only_pending: bool,
    situation_filter: productions::SituationFilter,
    selected_op: Option<i64>,

    // "Tipos de documento" tab.
    new_doc_code: String,
    new_doc_name: String,
}

#[derive(Debug, Clone)]
enum Message {
    UsernameChanged(String),
    PasswordChanged(String),
    ConfirmChanged(String),
    ShowPasswordToggled(bool),
    FocusNext(&'static str),
    Submit,
    Register,
    GoToRegister,
    GoToLogin,
    Logout,
    TabSelected(Tab),

    // Users & permissions.
    SetStatus(String, Status),
    Remove(String),
    UserSelected(String),
    PermissionToggled(String, &'static str, bool),
    SetAllPermissions(String, bool),

    // Document types & requirements.
    DocCodeChanged(String),
    DocNameChanged(String),
    AddDocType,
    GeneralToggled(DocId, bool),
    ProductsRefresh,
    ProductsLoaded(Result<Vec<Product>, String>),
    ProductSearchChanged(String),
    ProductSelected(i64),
    SpecificToggled(i64, DocId, bool),
    SpecificTabSelected(SpecificTab),

    // Groups.
    GroupSelected(GroupId),
    NewGroupNameChanged(String),
    GroupSearchChanged(String),
    AddToGroup(GroupId, i64),

    // Confirmation pop-up.
    Request(Pending),
    DialogNameChanged(String),
    DialogUsernameChanged(String),
    DialogPasswordChanged(String),
    DialogConfirm,
    DialogCancel,

    // Productions & attachments.
    PeriodFromChanged(String),
    PeriodToChanged(String),
    ProductionsRefresh,
    ProductionsLoaded(Result<Vec<Production>, String>),
    ProductionSearchChanged(String),
    OnlyPendingToggled(bool),
    SituationFilterChanged(productions::SituationFilter),
    OpSelected(i64),
    Attach { op: i64, unit: Unit, doc: DocId },
    AttachPicked { op: i64, unit: Unit, doc: DocId, path: Option<PathBuf> },
    OpenFile(PathBuf),
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let store = UserStore::load().unwrap_or_else(|e| {
            eprintln!("Não foi possível carregar os usuários: {e}");
            std::process::exit(1);
        });
        let quality = QualityStore::load().unwrap_or_else(|e| {
            eprintln!("Não foi possível carregar os dados da qualidade: {e}");
            std::process::exit(1);
        });

        let today = chrono::Local::now().date_naive();
        let from = today - chrono::Duration::days(DEFAULT_PERIOD_DAYS);

        let app = Self {
            store,
            quality,
            screen: Screen::default(),
            session: None,
            tab: Tab::default(),
            notice: None,
            dialog: None,
            username: String::new(),
            password: String::new(),
            confirm: String::new(),
            show_password: false,
            selected_user: None,
            products: erp::load_cached_products(),
            products_status: Remote::Idle,
            product_search: String::new(),
            selected_product: None,
            specific_tab: SpecificTab::default(),
            selected_group: None,
            new_group_name: String::new(),
            group_search: String::new(),
            productions: Remote::Idle,
            period_from: from.format("%d/%m/%Y").to_string(),
            period_to: today.format("%d/%m/%Y").to_string(),
            production_search: String::new(),
            only_pending: false,
            situation_filter: productions::SituationFilter::default(),
            selected_op: None,
            new_doc_code: String::new(),
            new_doc_name: String::new(),
        };

        (app, operation::focus(USERNAME_ID))
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNight
    }

    /// Whether the logged-in user holds a permission. The administrator holds all.
    fn can(&self, permission: &str) -> bool {
        self.session.as_ref().is_some_and(|session| {
            session.is_admin
                || self
                    .store
                    .get(&session.username)
                    .is_some_and(|user| user.can(permission))
        })
    }

    fn can_see(&self, tab: Tab) -> bool {
        match tab.permission() {
            Some(permission) => self.can(permission),
            None => self.session.as_ref().is_some_and(|s| s.is_admin),
        }
    }

    fn visible_tabs(&self) -> impl Iterator<Item = Tab> + '_ {
        Tab::ALL.into_iter().filter(|tab| self.can_see(*tab))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UsernameChanged(value) => {
                self.username = value;
                self.clear_error();
            }
            Message::PasswordChanged(value) => {
                self.password = value;
                self.clear_error();
            }
            Message::ConfirmChanged(value) => {
                self.confirm = value;
                self.clear_error();
            }
            Message::ShowPasswordToggled(show) => self.show_password = show,
            Message::FocusNext(id) => return operation::focus(id),
            Message::Submit => return self.login(),
            Message::Register => return self.register(),
            Message::GoToRegister => return self.go_to(Screen::Register, None),
            Message::GoToLogin => return self.go_to(Screen::Login, None),
            Message::Logout => {
                self.session = None;
                return self.go_to(Screen::Login, None);
            }
            Message::TabSelected(tab) => return self.select_tab(tab),

            Message::SetStatus(username, status) if self.is_admin() => {
                if status != Status::Approved {
                    self.deselect(&username);
                }
                let result = self.store.set_status(&username, status);
                self.report(result);
            }
            Message::Remove(username) if self.is_admin() => {
                self.deselect(&username);
                let result = self.store.remove(&username);
                self.report(result);
            }
            Message::UserSelected(username) => self.selected_user = Some(username),
            Message::PermissionToggled(username, permission, granted) if self.is_admin() => {
                let result = self.store.set_permission(&username, permission, granted);
                self.report(result);
            }
            Message::SetAllPermissions(username, granted) if self.is_admin() => {
                let result = self.store.set_all_permissions(&username, granted);
                self.report(result);
            }

            Message::DocCodeChanged(value) => self.new_doc_code = value,
            Message::DocNameChanged(value) => self.new_doc_name = value,
            Message::AddDocType if self.can(permissions::DOCUMENTS_CONFIGURE) => {
                match self.quality.add_doc_type(&self.new_doc_code, &self.new_doc_name) {
                    Ok(()) => {
                        self.new_doc_code.clear();
                        self.new_doc_name.clear();
                        self.notice = None;
                    }
                    Err(e) => self.notice = Some(Notice::Error(e)),
                }
            }
            Message::GeneralToggled(id, required) if self.can(permissions::DOCUMENTS_CONFIGURE) => {
                let result = self.quality.set_general(id, required);
                self.report(result);
            }
            Message::ProductsRefresh => return self.fetch_products(),
            Message::ProductsLoaded(result) => match result {
                Ok(products) => {
                    self.products = products;
                    self.products_status = Remote::Loaded(());
                }
                Err(e) => self.products_status = Remote::Failed(e),
            },
            Message::ProductSearchChanged(value) => self.product_search = value,
            Message::ProductSelected(code) => self.selected_product = Some(code),
            Message::SpecificToggled(product, id, required)
                if self.can(permissions::DOCUMENTS_CONFIGURE) =>
            {
                if !required {
                    return self.open_dialog(Pending::RemoveSpecific(product, id));
                }
                let result = self.quality.set_specific(product, id, true);
                self.report(result);
            }
            Message::SpecificTabSelected(tab) => {
                self.specific_tab = tab;
                self.notice = None;
            }

            Message::GroupSelected(id) => self.selected_group = Some(id),
            Message::NewGroupNameChanged(value) => self.new_group_name = value,
            Message::GroupSearchChanged(value) => self.group_search = value,
            Message::AddToGroup(id, product) if self.can(permissions::DOCUMENTS_CONFIGURE) => {
                let result = self.quality.add_to_group(id, product);
                self.report(result);
            }

            Message::Request(action) if self.can(permissions::DOCUMENTS_CONFIGURE) => {
                return self.open_dialog(action);
            }
            Message::DialogNameChanged(value) => {
                if let Some(dialog) = &mut self.dialog {
                    dialog.name = value;
                }
            }
            Message::DialogUsernameChanged(value) => {
                if let Some(dialog) = &mut self.dialog {
                    dialog.username = value;
                }
            }
            Message::DialogPasswordChanged(value) => {
                if let Some(dialog) = &mut self.dialog {
                    dialog.password = value;
                }
            }
            Message::DialogConfirm => return self.confirm_dialog(),
            Message::DialogCancel => self.dialog = None,

            Message::PeriodFromChanged(value) => self.period_from = value,
            Message::PeriodToChanged(value) => self.period_to = value,
            Message::ProductionsRefresh => return self.fetch_productions(),
            Message::ProductionsLoaded(result) => {
                self.productions = match result {
                    Ok(list) => Remote::Loaded(list),
                    Err(e) => Remote::Failed(e),
                };
            }
            Message::ProductionSearchChanged(value) => self.production_search = value,
            Message::OnlyPendingToggled(value) => self.only_pending = value,
            Message::SituationFilterChanged(filter) => self.situation_filter = filter,
            Message::OpSelected(op) => self.selected_op = Some(op),
            Message::Attach { op, unit, doc } if self.can(permissions::PRODUCTIONS_ATTACH) => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Selecione o PDF do documento")
                            .add_filter("PDF", &["pdf"])
                            .pick_file()
                            .await
                            .map(|file| file.path().to_path_buf())
                    },
                    move |path| Message::AttachPicked { op, unit: unit.clone(), doc, path },
                );
            }
            Message::AttachPicked { op, unit, doc, path: Some(path) }
                if self.can(permissions::PRODUCTIONS_ATTACH) =>
            {
                let user = self.session.as_ref().map(|s| s.username.clone()).unwrap_or_default();
                self.notice = Some(match self.quality.attach(op, &unit, doc, &path, &user) {
                    Ok(version) => Notice::Info(format!("PDF anexado como Rev. {version:02}.")),
                    Err(e) => Notice::Error(e),
                });
            }
            Message::OpenFile(path) => {
                if let Err(e) = open::that_detached(&path) {
                    self.notice = Some(Notice::Error(format!("Não foi possível abrir o PDF: {e}")));
                }
            }

            // Actions the current user is not authorized to perform, and a
            // cancelled file dialog.
            Message::SetStatus(..)
            | Message::Remove(_)
            | Message::PermissionToggled(..)
            | Message::SetAllPermissions(..)
            | Message::AddDocType
            | Message::GeneralToggled(..)
            | Message::SpecificToggled(..)
            | Message::AddToGroup(..)
            | Message::Request(_)
            | Message::Attach { .. }
            | Message::AttachPicked { .. } => {}
        }

        Task::none()
    }

    fn is_admin(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.is_admin)
    }

    fn open_dialog(&mut self, action: Pending) -> Task<Message> {
        let product_name = |code: i64| {
            self.products
                .iter()
                .find(|p| p.codigo == code)
                .map_or_else(|| format!("produto {code}"), |p| p.descricao.clone())
        };
        let doc_name = |id: DocId| {
            self.quality
                .doc_type(id)
                .map_or_else(String::new, |d| format!("{} — {}", d.code, d.name))
        };
        let group_name = |id: GroupId| {
            self.quality.group(id).map_or_else(String::new, |g| g.name.clone())
        };

        let (title, message, name) = match &action {
            Pending::CreateGroup { op: Some(op), products } => (
                "Criar grupo".to_owned(),
                format!(
                    "Criar um grupo com os {} produtos da OP {op}? Nas OPs em que mais de um \
                     produto do grupo aparecer, os documentos serão exigidos uma única vez.",
                    products.len()
                ),
                format!("Grupo OP {op}"),
            ),
            Pending::CreateGroup { op: None, .. } => (
                "Criar grupo".to_owned(),
                "Criar este grupo? Depois você pode adicionar os produtos a ele.".to_owned(),
                self.new_group_name.trim().to_owned(),
            ),
            Pending::DeleteGroup(id) => (
                "Excluir grupo".to_owned(),
                format!(
                    "Excluir o grupo {}? Os PDFs já anexados continuam valendo para cada produto.",
                    group_name(*id)
                ),
                String::new(),
            ),
            Pending::RemoveFromGroup(id, product) => (
                "Remover do grupo".to_owned(),
                format!("Remover {} do grupo {}?", product_name(*product), group_name(*id)),
                String::new(),
            ),
            Pending::DeleteDocType(id) => (
                "Excluir tipo de documento".to_owned(),
                format!("Excluir o documento {}? Ele deixa de ser exigido em todas as OPs.", doc_name(*id)),
                String::new(),
            ),
            Pending::RemoveSpecific(product, id) => (
                "Remover necessidade".to_owned(),
                format!(
                    "Deixar de exigir {} para {}?",
                    doc_name(*id),
                    product_name(*product)
                ),
                String::new(),
            ),
        };

        let focus = if action.is_deletion() { DIALOG_USERNAME_ID } else { DIALOG_NAME_ID };
        self.dialog = Some(Dialog {
            action,
            title,
            message,
            name,
            username: String::new(),
            password: String::new(),
            error: None,
        });
        operation::focus(focus)
    }

    fn confirm_dialog(&mut self) -> Task<Message> {
        let Some(dialog) = &self.dialog else {
            return Task::none();
        };
        if !self.can(permissions::DOCUMENTS_CONFIGURE) {
            self.dialog = None;
            return Task::none();
        }

        if dialog.action.is_deletion() && !self.verify_current_user(&dialog.username, &dialog.password) {
            if let Some(dialog) = &mut self.dialog {
                dialog.password.clear();
                dialog.error = Some("Usuário ou senha incorretos.".into());
            }
            return operation::focus(DIALOG_PASSWORD_ID);
        }

        let name = dialog.name.clone();
        let result = match dialog.action.clone() {
            Pending::CreateGroup { op, products } => {
                self.quality.create_group(&name, products).map(|id| {
                    self.selected_group = Some(id);
                    if op.is_none() {
                        self.new_group_name.clear();
                    }
                    format!("Grupo {} criado.", name.trim())
                })
            }
            Pending::DeleteGroup(id) => self.quality.delete_group(id).map(|()| {
                if self.selected_group == Some(id) {
                    self.selected_group = None;
                }
                "Grupo excluído.".to_owned()
            }),
            Pending::RemoveFromGroup(id, product) => self
                .quality
                .remove_from_group(id, product)
                .map(|()| "Produto removido do grupo.".to_owned()),
            Pending::DeleteDocType(id) => self
                .quality
                .remove_doc_type(id)
                .map(|()| "Tipo de documento excluído.".to_owned()),
            Pending::RemoveSpecific(product, id) => self
                .quality
                .set_specific(product, id, false)
                .map(|()| "Necessidade removida.".to_owned()),
        };

        match result {
            Ok(message) => {
                self.dialog = None;
                self.notice = Some(Notice::Info(message));
            }
            Err(e) => {
                if let Some(dialog) = &mut self.dialog {
                    dialog.error = Some(e);
                }
            }
        }
        Task::none()
    }

    /// Checks that the typed credentials belong to the user who is logged in.
    fn verify_current_user(&self, username: &str, password: &str) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let username = username.trim();
        if !username.eq_ignore_ascii_case(&session.username) {
            return false;
        }
        match self.store.login(username, password) {
            LoginResult::Admin => session.is_admin,
            LoginResult::User => !session.is_admin,
            _ => false,
        }
    }

    fn login(&mut self) -> Task<Message> {
        let username = self.username.trim().to_owned();

        if username.is_empty() || self.password.is_empty() {
            return self.error("Informe usuário e senha.");
        }

        let is_admin = match self.store.login(&username, &self.password) {
            LoginResult::Admin => true,
            LoginResult::User => false,
            LoginResult::Pending => {
                return self.error("Seu cadastro ainda aguarda aprovação do administrador.");
            }
            LoginResult::Rejected => return self.error("Seu cadastro foi recusado."),
            LoginResult::Invalid => {
                self.password.clear();
                self.notice = Some(Notice::Error("Usuário ou senha inválidos.".into()));
                return operation::focus(PASSWORD_ID);
            }
        };

        let username = if is_admin { auth::ADMIN_USERNAME.to_owned() } else { username };
        self.session = Some(Session { username, is_admin });
        let _ = self.go_to(Screen::Main, None);

        let first = self.visible_tabs().next().unwrap_or_default();
        self.select_tab(first)
    }

    fn register(&mut self) -> Task<Message> {
        if self.password != self.confirm {
            self.confirm.clear();
            self.notice = Some(Notice::Error("As senhas não coincidem.".into()));
            return operation::focus(CONFIRM_ID);
        }

        match self.store.register(self.username.trim(), &self.password) {
            Ok(()) => self.go_to(
                Screen::Login,
                Some(Notice::Info(
                    "Cadastro enviado! Aguarde a aprovação do administrador.".into(),
                )),
            ),
            Err(e) => self.error(e),
        }
    }

    fn select_tab(&mut self, tab: Tab) -> Task<Message> {
        if !self.can_see(tab) {
            return Task::none();
        }
        self.tab = tab;
        self.notice = None;

        // Load ERP data the first time a tab that needs it is opened.
        match tab {
            Tab::Productions if matches!(self.productions, Remote::Idle) => self.fetch_productions(),
            Tab::SpecificNeeds
                if self.products.is_empty() && matches!(self.products_status, Remote::Idle) =>
            {
                self.fetch_products()
            }
            _ => Task::none(),
        }
    }

    fn fetch_products(&mut self) -> Task<Message> {
        if self.products_status.is_loading() {
            return Task::none();
        }
        self.products_status = Remote::Loading;
        Task::perform(erp::fetch_products(), Message::ProductsLoaded)
    }

    fn fetch_productions(&mut self) -> Task<Message> {
        if self.productions.is_loading() {
            return Task::none();
        }
        let (Some(from), Some(to)) = (
            widgets::iso_date(&self.period_from),
            widgets::iso_date(&self.period_to),
        ) else {
            self.productions =
                Remote::Failed("Informe as datas do período no formato DD/MM/AAAA.".into());
            return Task::none();
        };
        if from > to {
            self.productions =
                Remote::Failed("A data inicial é posterior à data final.".into());
            return Task::none();
        }

        self.productions = Remote::Loading;
        Task::perform(erp::fetch_productions(from, to), Message::ProductionsLoaded)
    }

    /// Switches screens, clearing the form fields.
    fn go_to(&mut self, screen: Screen, notice: Option<Notice>) -> Task<Message> {
        self.screen = screen;
        self.username.clear();
        self.password.clear();
        self.confirm.clear();
        self.show_password = false;
        self.notice = notice;
        self.dialog = None;
        self.selected_user = None;
        operation::focus(USERNAME_ID)
    }

    fn error(&mut self, message: impl Into<String>) -> Task<Message> {
        self.notice = Some(Notice::Error(message.into()));
        Task::none()
    }

    fn report(&mut self, result: Result<(), String>) {
        self.notice = result.err().map(Notice::Error);
    }

    fn deselect(&mut self, username: &str) {
        if self.selected_user.as_deref() == Some(username) {
            self.selected_user = None;
        }
    }

    fn clear_error(&mut self) {
        if matches!(self.notice, Some(Notice::Error(_))) {
            self.notice = None;
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let screen = match self.screen {
            Screen::Login => self.login_view(),
            Screen::Register => self.register_view(),
            Screen::Main => self.main_view(),
        };

        match &self.dialog {
            Some(dialog) => widgets::modal(screen, self.dialog_view(dialog)),
            None => screen,
        }
    }

    fn dialog_view<'a>(&'a self, dialog: &'a Dialog) -> Element<'a, Message> {
        use iced::widget::{button, text_input};

        let mut content = column![
            text(&dialog.title).size(20),
            text(&dialog.message).size(14),
        ]
        .spacing(14);

        if let Pending::CreateGroup { .. } = dialog.action {
            content = content.push(
                column![
                    text("Nome do grupo").size(13),
                    text_input("Ex.: Lentes retangulares", &dialog.name)
                        .id(DIALOG_NAME_ID)
                        .on_input(Message::DialogNameChanged)
                        .on_submit(Message::DialogConfirm)
                        .padding(8),
                ]
                .spacing(6),
            );
        }

        if dialog.action.is_deletion() {
            content = content.push(widgets::hint(
                "Para confirmar a exclusão, informe novamente seu usuário e senha.",
            ));
            content = content.push(
                text_input("Usuário", &dialog.username)
                    .id(DIALOG_USERNAME_ID)
                    .on_input(Message::DialogUsernameChanged)
                    .on_submit(Message::FocusNext(DIALOG_PASSWORD_ID))
                    .padding(8),
            );
            content = content.push(
                text_input("Senha", &dialog.password)
                    .id(DIALOG_PASSWORD_ID)
                    .secure(true)
                    .on_input(Message::DialogPasswordChanged)
                    .on_submit(Message::DialogConfirm)
                    .padding(8),
            );
        }

        if let Some(error) = &dialog.error {
            content = content.push(text(error).size(14).style(text::danger));
        }

        let (label, style): (&str, widgets::ButtonStyle) = if dialog.action.is_deletion() {
            ("Excluir", button::danger)
        } else {
            ("Confirmar", button::primary)
        };

        content = content.push(
            row![
                space::horizontal(),
                action_button("Cancelar", Message::DialogCancel, button::secondary),
                action_button(label, Message::DialogConfirm, style),
            ]
            .spacing(8),
        );

        container(content)
            .padding(24)
            .width(Length::Fixed(440.0))
            .style(container::rounded_box)
            .into()
    }

    fn main_view(&self) -> Element<'_, Message> {
        let Some(session) = &self.session else {
            return self.login_view();
        };

        let tabs = row(self
            .visible_tabs()
            .map(|tab| tab_button(tab.label(), Message::TabSelected(tab), tab == self.tab)))
        .spacing(6)
        .wrap();

        let role = if session.is_admin { "Administrador" } else { "Usuário" };
        let top_bar = row![
            text("SGQ").size(24),
            space::horizontal(),
            column![
                text(&session.username).size(14),
                text(role).size(12).style(text::secondary),
            ]
            .align_x(Alignment::End),
            action_button("Sair", Message::Logout, iced::widget::button::secondary),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let body = if !self.can_see(self.tab) {
            container(widgets::hint(
                "Você não tem autorização para acessar nenhuma seção. Contate o administrador.",
            ))
            .center(Length::Fill)
            .into()
        } else {
            match self.tab {
                Tab::Home => self.home_tab(session),
                Tab::Productions => self.productions_tab(),
                Tab::DocTypes => self.doc_types_tab(),
                Tab::GeneralNeeds => self.general_needs_tab(),
                Tab::SpecificNeeds => self.specific_needs_tab(),
                Tab::Users => self.users_tab(),
                Tab::Permissions => self.permissions_tab(),
            }
        };

        column![top_bar, tabs, rule::horizontal(1)]
            .push(self.notice_view())
            .push(body)
            .spacing(14)
            .padding(20)
            .into()
    }

    fn home_tab<'a>(&'a self, session: &'a Session) -> Element<'a, Message> {
        let content = column![
            text(format!("Olá, {}!", session.username)).size(28),
            widgets::hint("Use as abas acima para navegar pelo sistema."),
        ]
        .spacing(12)
        .align_x(Alignment::Center);

        container(content).center(Length::Fill).into()
    }

    fn notice_view(&self) -> Option<Element<'_, Message>> {
        self.notice.as_ref().map(|notice| match notice {
            Notice::Error(msg) => text(msg).size(14).style(text::danger).into(),
            Notice::Info(msg) => text(msg).size(14).style(text::success).into(),
        })
    }
}
