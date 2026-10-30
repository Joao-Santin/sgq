//! Quality-control documents: document types, which ones each product requires,
//! product groups that share documents, and the PDFs attached to each
//! product (or group) of each production order (OP).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const DATA_FILE: &str = "qualidade.json";
const ATTACHMENTS_DIR: &str = "anexos";

pub type DocId = u32;
pub type GroupId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocType {
    pub id: DocId,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub op: i64,
    pub product: i64,
    /// Other products of the same OP this PDF also covers (when attached to a group).
    #[serde(default)]
    pub covers: Vec<i64>,
    pub doc: DocId,
    pub version: u32,
    /// Path relative to the attachments directory.
    pub file: String,
    pub original_name: String,
    pub uploaded_by: String,
    pub uploaded_at: String,
}

impl Attachment {
    fn covers_product(&self, product: i64) -> bool {
        self.product == product || self.covers.contains(&product)
    }
}

/// Products that share the same quality documents (e.g. color variations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub products: BTreeSet<i64>,
}

/// What documents are required for within an OP: a single product, or the
/// products of a group when two or more of them are in the same OP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub group: Option<GroupId>,
    pub products: Vec<i64>,
}

#[derive(Default, Serialize, Deserialize)]
struct Data {
    next_id: DocId,
    doc_types: Vec<DocType>,
    /// Required for every product of every OP.
    general: BTreeSet<DocId>,
    /// Extra documents required only for specific products (by ERP product code).
    specific: BTreeMap<i64, BTreeSet<DocId>>,
    attachments: Vec<Attachment>,
    #[serde(default)]
    next_group_id: GroupId,
    #[serde(default)]
    groups: Vec<Group>,
}

pub struct QualityStore {
    data: Data,
    path: PathBuf,
    attachments_dir: PathBuf,
}

impl Default for QualityStore {
    fn default() -> Self {
        Self {
            data: Data::default(),
            path: PathBuf::from(DATA_FILE),
            attachments_dir: PathBuf::from(ATTACHMENTS_DIR),
        }
    }
}

impl QualityStore {
    pub fn load() -> io::Result<Self> {
        let store = Self::default();

        let data = match fs::read_to_string(&store.path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e),
        };

        Ok(Self { data, ..store })
    }

    fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|e| format!("Erro ao salvar dados da qualidade: {e}"))?;
        fs::write(&self.path, json).map_err(|e| format!("Erro ao salvar dados da qualidade: {e}"))
    }

    pub fn doc_types(&self) -> &[DocType] {
        &self.data.doc_types
    }

    pub fn doc_type(&self, id: DocId) -> Option<&DocType> {
        self.data.doc_types.iter().find(|d| d.id == id)
    }

    pub fn add_doc_type(&mut self, code: &str, name: &str) -> Result<(), String> {
        let (code, name) = (code.trim(), name.trim());
        if code.is_empty() || name.is_empty() {
            return Err("Informe o código e o nome do documento.".into());
        }
        if self.data.doc_types.iter().any(|d| d.code.eq_ignore_ascii_case(code)) {
            return Err(format!("Já existe um documento com o código {code}."));
        }

        self.data.next_id += 1;
        self.data.doc_types.push(DocType {
            id: self.data.next_id,
            code: code.to_owned(),
            name: name.to_owned(),
        });
        self.save()
    }

    /// Removes a document type. Refused while PDFs are attached to it, so no
    /// quality record is ever orphaned.
    pub fn remove_doc_type(&mut self, id: DocId) -> Result<(), String> {
        let attached = self.data.attachments.iter().filter(|a| a.doc == id).count();
        if attached > 0 {
            return Err(format!(
                "Este documento tem {attached} PDF(s) anexado(s) e não pode ser removido."
            ));
        }

        self.data.doc_types.retain(|d| d.id != id);
        self.data.general.remove(&id);
        for docs in self.data.specific.values_mut() {
            docs.remove(&id);
        }
        self.data.specific.retain(|_, docs| !docs.is_empty());
        self.save()
    }

    pub fn is_general(&self, id: DocId) -> bool {
        self.data.general.contains(&id)
    }

    pub fn set_general(&mut self, id: DocId, required: bool) -> Result<(), String> {
        if required {
            self.data.general.insert(id);
        } else {
            self.data.general.remove(&id);
        }
        self.save()
    }

    pub fn is_specific(&self, product: i64, id: DocId) -> bool {
        self.data
            .specific
            .get(&product)
            .is_some_and(|docs| docs.contains(&id))
    }

    pub fn specific_count(&self, product: i64) -> usize {
        self.data.specific.get(&product).map_or(0, BTreeSet::len)
    }

    pub fn set_specific(&mut self, product: i64, id: DocId, required: bool) -> Result<(), String> {
        let docs = self.data.specific.entry(product).or_default();
        if required {
            docs.insert(id);
        } else {
            docs.remove(&id);
            if docs.is_empty() {
                self.data.specific.remove(&product);
            }
        }
        self.save()
    }

    /// Documents required for a product: general ones plus its specific ones.
    pub fn required(&self, product: i64) -> BTreeSet<DocId> {
        let mut docs = self.data.general.clone();
        if let Some(specific) = self.data.specific.get(&product) {
            docs.extend(specific);
        }
        docs.retain(|id| self.doc_type(*id).is_some());
        docs
    }

    pub fn groups(&self) -> &[Group] {
        &self.data.groups
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.data.groups.iter().find(|g| g.id == id)
    }

    pub fn group_of(&self, product: i64) -> Option<&Group> {
        self.data.groups.iter().find(|g| g.products.contains(&product))
    }

    /// Creates a group. A product can belong to only one group.
    pub fn create_group(
        &mut self,
        name: &str,
        products: impl IntoIterator<Item = i64>,
    ) -> Result<GroupId, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Informe o nome do grupo.".into());
        }
        if self.data.groups.iter().any(|g| g.name.eq_ignore_ascii_case(name)) {
            return Err(format!("Já existe um grupo chamado {name}."));
        }

        let products: BTreeSet<i64> = products.into_iter().collect();
        let taken: BTreeSet<String> = products
            .iter()
            .filter_map(|p| self.group_of(*p).map(|g| g.name.clone()))
            .collect();
        if !taken.is_empty() {
            return Err(format!(
                "Alguns produtos já pertencem a outro grupo: {}.",
                taken.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }

        self.data.next_group_id += 1;
        let id = self.data.next_group_id;
        self.data.groups.push(Group { id, name: name.to_owned(), products });
        self.save()?;
        Ok(id)
    }

    /// Deletes a group. PDFs already attached to it keep counting for each of
    /// the products they cover.
    pub fn delete_group(&mut self, id: GroupId) -> Result<(), String> {
        self.data.groups.retain(|g| g.id != id);
        self.save()
    }

    pub fn add_to_group(&mut self, id: GroupId, product: i64) -> Result<(), String> {
        if let Some(other) = self.group_of(product) {
            return Err(format!("Este produto já pertence ao grupo {}.", other.name));
        }
        if let Some(group) = self.data.groups.iter_mut().find(|g| g.id == id) {
            group.products.insert(product);
        }
        self.save()
    }

    pub fn remove_from_group(&mut self, id: GroupId, product: i64) -> Result<(), String> {
        if let Some(group) = self.data.groups.iter_mut().find(|g| g.id == id) {
            group.products.remove(&product);
        }
        self.save()
    }

    /// Splits the products of an OP into documentation units: products of the
    /// same group are merged when two or more of them are in the OP.
    pub fn units(&self, products: impl IntoIterator<Item = i64>) -> Vec<Unit> {
        let mut distinct: Vec<i64> = Vec::new();
        for product in products {
            if !distinct.contains(&product) {
                distinct.push(product);
            }
        }

        let group_of = |p: i64| self.group_of(p).map(|g| g.id);
        let mut units: Vec<Unit> = Vec::new();
        for &product in &distinct {
            let group = group_of(product);
            let in_op = group.map_or(0, |g| distinct.iter().filter(|p| group_of(**p) == Some(g)).count());

            match group {
                Some(g) if in_op > 1 => {
                    if let Some(unit) = units.iter_mut().find(|u| u.group == Some(g)) {
                        unit.products.push(product);
                    } else {
                        units.push(Unit { group: Some(g), products: vec![product] });
                    }
                }
                _ => units.push(Unit { group: None, products: vec![product] }),
            }
        }
        units
    }

    /// Documents required for a unit: everything any of its products requires.
    pub fn unit_required(&self, unit: &Unit) -> BTreeSet<DocId> {
        unit.products.iter().flat_map(|p| self.required(*p)).collect()
    }

    /// Required documents of a unit in an OP that have no PDF attached yet.
    pub fn pending(&self, op: i64, unit: &Unit) -> usize {
        self.unit_required(unit)
            .into_iter()
            .filter(|doc| self.versions(op, unit, *doc).is_empty())
            .count()
    }

    /// Documents shown for a unit in an OP: required ones plus any other
    /// document that already has PDFs attached.
    pub fn docs_for(&self, op: i64, unit: &Unit) -> BTreeSet<DocId> {
        let mut docs = self.unit_required(unit);
        docs.extend(
            self.data
                .attachments
                .iter()
                .filter(|a| a.op == op && unit.products.iter().any(|p| a.covers_product(*p)))
                .map(|a| a.doc),
        );
        docs
    }

    /// All versions attached for a document of a unit, newest first.
    pub fn versions(&self, op: i64, unit: &Unit, doc: DocId) -> Vec<&Attachment> {
        let mut versions: Vec<&Attachment> = self
            .data
            .attachments
            .iter()
            .filter(|a| {
                a.op == op && a.doc == doc && unit.products.iter().any(|p| a.covers_product(*p))
            })
            .collect();
        versions.sort_by_key(|a| std::cmp::Reverse(a.version));
        versions
    }

    /// Copies `source` into the attachments directory as a new version of the document.
    pub fn attach(
        &mut self,
        op: i64,
        unit: &Unit,
        doc: DocId,
        source: &Path,
        user: &str,
    ) -> Result<u32, String> {
        let doc_type = self
            .doc_type(doc)
            .ok_or("Tipo de documento não encontrado.")?
            .clone();
        let (&product, covers) = unit
            .products
            .split_first()
            .ok_or("Nenhum produto selecionado.")?;

        if !is_pdf(source) {
            return Err("O arquivo selecionado não é um PDF válido.".into());
        }

        let version = self
            .versions(op, unit, doc)
            .first()
            .map_or(1, |latest| latest.version + 1);

        let folder = match unit.group {
            Some(group) => format!("grupo_{group}"),
            None => format!("produto_{product}"),
        };
        let base = PathBuf::from(format!("OP_{op}")).join(folder);
        let stem = format!("{}_rev{version:02}", sanitize(&doc_type.code));

        // Never overwrite an earlier PDF (possible if group members changed).
        let mut relative = base.join(format!("{stem}.pdf"));
        let mut copy = 1;
        while self.attachments_dir.join(&relative).exists() {
            copy += 1;
            relative = base.join(format!("{stem}_{copy}.pdf"));
        }
        let destination = self.attachments_dir.join(&relative);

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Erro ao criar pasta de anexos: {e}"))?;
        }
        fs::copy(source, &destination).map_err(|e| format!("Erro ao copiar o PDF: {e}"))?;

        self.data.attachments.push(Attachment {
            op,
            product,
            covers: covers.to_vec(),
            doc,
            version,
            file: relative.to_string_lossy().replace('\\', "/"),
            original_name: source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            uploaded_by: user.to_owned(),
            uploaded_at: chrono::Local::now().format("%d/%m/%Y %H:%M").to_string(),
        });

        if let Err(e) = self.save() {
            let _ = fs::remove_file(&destination);
            self.data.attachments.pop();
            return Err(e);
        }

        Ok(version)
    }

    pub fn file_path(&self, attachment: &Attachment) -> PathBuf {
        self.attachments_dir.join(&attachment.file)
    }
}

fn is_pdf(path: &Path) -> bool {
    let mut header = [0u8; 5];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok_and(|()| &header == b"%PDF-")
}

fn sanitize(code: &str) -> String {
    code.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> QualityStore {
        let dir = std::env::temp_dir().join(format!("sgq_quality_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        QualityStore {
            data: Data::default(),
            path: dir.join(DATA_FILE),
            attachments_dir: dir.join(ATTACHMENTS_DIR),
        }
    }

    fn single(product: i64) -> Unit {
        Unit { group: None, products: vec![product] }
    }

    #[test]
    fn requirements_and_versions() {
        let mut store = temp_store("versions");
        store.add_doc_type("FCQ-01", "Inspeção inicial").unwrap();
        store.add_doc_type("FCQ-02", "Dimensional").unwrap();
        assert!(store.add_doc_type("fcq-01", "Duplicado").is_err());

        store.set_general(1, true).unwrap();
        store.set_specific(658, 2, true).unwrap();
        assert_eq!(store.required(658).len(), 2);
        assert_eq!(store.required(500).len(), 1);
        assert_eq!(store.pending(1326, &single(658)), 2);

        let pdf = store.path.with_file_name("ficha.pdf");
        fs::write(&pdf, b"%PDF-1.4 teste").unwrap();
        assert_eq!(store.attach(1326, &single(658), 1, &pdf, "jose").unwrap(), 1);
        assert_eq!(store.attach(1326, &single(658), 1, &pdf, "jose").unwrap(), 2);
        assert_eq!(store.pending(1326, &single(658)), 1);
        assert_eq!(store.versions(1326, &single(658), 1)[0].version, 2);
        assert!(store.file_path(store.versions(1326, &single(658), 1)[0]).exists());

        // Other OPs of the same product are tracked separately.
        assert_eq!(store.pending(1400, &single(658)), 2);

        assert!(store.remove_doc_type(1).is_err());
        store.remove_doc_type(2).unwrap();
        assert_eq!(store.pending(1326, &single(658)), 0);
    }

    #[test]
    fn groups_share_documents_within_an_op() {
        let mut store = temp_store("groups");
        store.add_doc_type("FCQ-01", "Inspeção").unwrap();
        store.set_general(1, true).unwrap();

        // Orange (658) and red (500) lenses share documents; 900 is unrelated.
        let group = store.create_group("Lentes retangulares", [658, 500]).unwrap();
        assert!(store.create_group("Outro", [658]).is_err());
        assert!(store.create_group("lentes RETANGULARES", [1]).is_err());

        let units = store.units([658, 500, 900, 658]);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0], Unit { group: Some(group), products: vec![658, 500] });
        assert_eq!(store.pending(1326, &units[0]), 1);

        // A single member of the group in an OP is documented on its own.
        assert_eq!(store.units([500, 900]), vec![single(500), single(900)]);

        let pdf = store.path.with_file_name("ficha.pdf");
        fs::write(&pdf, b"%PDF-1.4 teste").unwrap();
        store.attach(1326, &units[0], 1, &pdf, "jose").unwrap();
        assert_eq!(store.pending(1326, &units[0]), 0);

        // The group PDF still covers each product after the group is deleted.
        store.delete_group(group).unwrap();
        assert_eq!(store.pending(1326, &single(658)), 0);
        assert_eq!(store.pending(1326, &single(500)), 0);
        assert_eq!(store.pending(1326, &single(900)), 1);
    }

    #[test]
    fn earlier_product_pdfs_count_for_new_group() {
        let mut store = temp_store("late_group");
        store.add_doc_type("FCQ-01", "Inspeção").unwrap();
        store.set_general(1, true).unwrap();

        let pdf = store.path.with_file_name("ficha.pdf");
        fs::write(&pdf, b"%PDF-1.4 teste").unwrap();
        store.attach(1326, &single(658), 1, &pdf, "jose").unwrap();

        store.create_group("Lentes", [658, 500]).unwrap();
        let units = store.units([658, 500]);
        assert_eq!(units.len(), 1);
        assert_eq!(store.pending(1326, &units[0]), 0);
        assert_eq!(store.attach(1326, &units[0], 1, &pdf, "jose").unwrap(), 2);
    }

    #[test]
    fn rejects_non_pdf() {
        let mut store = temp_store("non_pdf");
        store.add_doc_type("FCQ-01", "Inspeção").unwrap();
        let file = store.path.with_file_name("planilha.xlsx");
        fs::write(&file, b"PK\x03\x04").unwrap();
        assert!(store.attach(1, &single(1), 1, &file, "jose").is_err());
    }
}
