//! Catalog of everything a user can be authorized to access or do.
//!
//! To add a new section to the system, declare its permission keys below and
//! add a `Section` entry to `SECTIONS`. The "Autorizações" tab picks it up
//! automatically.

use std::collections::BTreeSet;

pub const HOME_ACCESS: &str = "inicio.acessar";
pub const PRODUCTIONS_VIEW: &str = "producoes.visualizar";
pub const PRODUCTIONS_ATTACH: &str = "producoes.anexar";
pub const DOCUMENTS_CONFIGURE: &str = "documentos.configurar";

pub struct Permission {
    /// Stable identifier persisted in `users.json`. Never rename an existing key.
    pub key: &'static str,
    pub label: &'static str,
    /// Granted automatically to newly registered users.
    pub default: bool,
}

pub struct Section {
    pub name: &'static str,
    pub permissions: &'static [Permission],
}

pub const SECTIONS: &[Section] = &[
    Section {
        name: "Página inicial",
        permissions: &[Permission {
            key: HOME_ACCESS,
            label: "Acessar a página inicial",
            default: true,
        }],
    },
    Section {
        name: "Produções (OPs)",
        permissions: &[
            Permission {
                key: PRODUCTIONS_VIEW,
                label: "Ver produções e documentos anexados",
                default: false,
            },
            Permission {
                key: PRODUCTIONS_ATTACH,
                label: "Anexar PDFs às produções",
                default: false,
            },
        ],
    },
    Section {
        name: "Documentos da qualidade",
        permissions: &[Permission {
            key: DOCUMENTS_CONFIGURE,
            label: "Cadastrar tipos de documento e necessidades",
            default: false,
        }],
    },
];

pub fn all() -> impl Iterator<Item = &'static Permission> {
    SECTIONS.iter().flat_map(|s| s.permissions)
}

pub fn defaults() -> BTreeSet<String> {
    all()
        .filter(|p| p.default)
        .map(|p| p.key.to_owned())
        .collect()
}
