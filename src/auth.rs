use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::PathBuf;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};

use crate::permissions;

/// Administrator account, hardcoded. Only the Argon2id hash of the password is stored.
pub const ADMIN_USERNAME: &str = "joao";
const ADMIN_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$k++gh8AphFPClp3pnD/2qQ$XSAgS2KX8FuINeqz4yWU70RkiqEar9C6Re7PN1LG/30";

const USERS_FILE: &str = "users.json";
const MIN_PASSWORD_LEN: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    password_hash: String,
    pub status: Status,
    /// Accounts saved before permissions existed get the defaults.
    #[serde(default = "permissions::defaults")]
    permissions: BTreeSet<String>,
}

impl User {
    pub fn can(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

pub enum LoginResult {
    Admin,
    User,
    Pending,
    Rejected,
    Invalid,
}

#[derive(Default)]
pub struct UserStore {
    users: Vec<User>,
    path: PathBuf,
}

impl UserStore {
    pub fn load() -> io::Result<Self> {
        let path = PathBuf::from(USERS_FILE);

        let users = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };

        Ok(Self { users, path })
    }

    fn save(&self) -> io::Result<()> {
        let json = serde_json::to_string_pretty(&self.users)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&self.path, json)
    }

    pub fn login(&self, username: &str, password: &str) -> LoginResult {
        if username.eq_ignore_ascii_case(ADMIN_USERNAME) {
            return if verify(password, ADMIN_PASSWORD_HASH) {
                LoginResult::Admin
            } else {
                LoginResult::Invalid
            };
        }

        match self.find(username) {
            Some(user) if verify(password, &user.password_hash) => match user.status {
                Status::Approved => LoginResult::User,
                Status::Pending => LoginResult::Pending,
                Status::Rejected => LoginResult::Rejected,
            },
            _ => LoginResult::Invalid,
        }
    }

    /// Creates a new account awaiting administrator approval.
    pub fn register(&mut self, username: &str, password: &str) -> Result<(), String> {
        if username.is_empty() {
            return Err("Informe um nome de usuário.".into());
        }
        if username.chars().any(char::is_whitespace) {
            return Err("O nome de usuário não pode conter espaços.".into());
        }
        if username.eq_ignore_ascii_case(ADMIN_USERNAME) || self.find(username).is_some() {
            return Err("Este nome de usuário já está em uso.".into());
        }
        if password.chars().count() < MIN_PASSWORD_LEN {
            return Err(format!(
                "A senha deve ter pelo menos {MIN_PASSWORD_LEN} caracteres."
            ));
        }

        self.users.push(User {
            username: username.to_owned(),
            password_hash: hash(password)?,
            status: Status::Pending,
            permissions: permissions::defaults(),
        });

        self.save().map_err(save_error)
    }

    pub fn set_status(&mut self, username: &str, status: Status) -> Result<(), String> {
        if let Some(user) = self.users.iter_mut().find(|u| u.username == username) {
            user.status = status;
        }
        self.save().map_err(save_error)
    }

    pub fn remove(&mut self, username: &str) -> Result<(), String> {
        self.users.retain(|u| u.username != username);
        self.save().map_err(save_error)
    }

    pub fn set_permission(
        &mut self,
        username: &str,
        permission: &str,
        granted: bool,
    ) -> Result<(), String> {
        if let Some(user) = self.users.iter_mut().find(|u| u.username == username) {
            if granted {
                user.permissions.insert(permission.to_owned());
            } else {
                user.permissions.remove(permission);
            }
        }
        self.save().map_err(save_error)
    }

    pub fn set_all_permissions(&mut self, username: &str, granted: bool) -> Result<(), String> {
        if let Some(user) = self.users.iter_mut().find(|u| u.username == username) {
            user.permissions = if granted {
                permissions::all().map(|p| p.key.to_owned()).collect()
            } else {
                BTreeSet::new()
            };
        }
        self.save().map_err(save_error)
    }

    pub fn get(&self, username: &str) -> Option<&User> {
        self.users.iter().find(|u| u.username == username)
    }

    pub fn with_status(&self, status: Status) -> impl Iterator<Item = &User> {
        self.users.iter().filter(move |u| u.status == status)
    }

    fn find(&self, username: &str) -> Option<&User> {
        self.users
            .iter()
            .find(|u| u.username.eq_ignore_ascii_case(username))
    }
}

fn hash(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("Erro ao gerar hash da senha: {e}"))
}

fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

fn save_error(e: io::Error) -> String {
    format!("Erro ao salvar usuários: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> UserStore {
        let path = std::env::temp_dir().join(format!("sgq_{name}_{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        UserStore {
            users: Vec::new(),
            path,
        }
    }

    #[test]
    fn admin_rejects_wrong_password() {
        let store = temp_store("admin");
        assert!(PasswordHash::new(ADMIN_PASSWORD_HASH).is_ok());
        assert!(matches!(store.login("joao", "errada"), LoginResult::Invalid));
    }

    #[test]
    fn registration_requires_approval() {
        let mut store = temp_store("registration");
        store.register("maria", "segredo1").unwrap();
        assert!(store.register("joao", "segredo1").is_err());
        assert!(store.register("MARIA", "segredo1").is_err());

        assert!(matches!(store.login("maria", "segredo1"), LoginResult::Pending));
        store.set_status("maria", Status::Approved).unwrap();
        assert!(matches!(store.login("maria", "segredo1"), LoginResult::User));
        assert!(matches!(store.login("maria", "outra"), LoginResult::Invalid));

        let _ = fs::remove_file(&store.path);
    }

    #[test]
    fn permissions() {
        let mut store = temp_store("permissions");
        store.register("ana", "segredo1").unwrap();
        assert!(store.get("ana").unwrap().can(permissions::HOME_ACCESS));

        store.set_permission("ana", permissions::HOME_ACCESS, false).unwrap();
        assert!(!store.get("ana").unwrap().can(permissions::HOME_ACCESS));

        store.set_all_permissions("ana", true).unwrap();
        assert!(permissions::all().all(|p| store.get("ana").unwrap().can(p.key)));

        let _ = fs::remove_file(&store.path);
    }

    #[test]
    fn legacy_users_get_default_permissions() {
        let json = r#"[{"username":"jose","password_hash":"x","status":"Approved"}]"#;
        let users: Vec<User> = serde_json::from_str(json).unwrap();
        assert!(users[0].can(permissions::HOME_ACCESS));
    }
}
