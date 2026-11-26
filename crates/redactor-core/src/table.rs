//! Tables: CSV files and spreadsheet sheets.
//!
//! A cell like `María Gómez` carries no hint that it is a name; its column
//! header does. The header row is found near the top of the table, columns
//! titled like people (`Nombre`, `Cliente`, `Proveedor`, `Teléfono`...) or
//! documents (`NIT`, `Cédula`, `Documento`...) are masked whole, and every
//! other cell goes through the regular engine. Numbers such as prices stay.

use std::collections::BTreeMap;

use crate::dictionary::fold_char;
use crate::finding::Category;
use crate::redactor::Redactor;

/// What a column holds, judged by its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    /// Names and contact details of people or companies.
    Person,
    /// Identity and tax numbers.
    Document,
}

/// Header words, folded (lower case, no accents), matched as whole words.
const PERSON_HEADERS: &[&str] = &[
    "acreedor",
    "apellido",
    "apellidos",
    "asociado",
    "beneficiario",
    "celular",
    "cliente",
    "contacto",
    "contador",
    "deudor",
    "direccion",
    "empleado",
    "nombre",
    "nombres",
    "persona",
    "proveedor",
    "razon social",
    "responsable",
    "socio",
    "telefono",
    "tercero",
    "titular",
    "vendedor",
    "address",
    "client",
    "contact",
    "customer",
    "employee",
    "name",
    "owner",
    "payee",
    "phone",
    "supplier",
    "vendor",
];

const DOCUMENT_HEADERS: &[&str] = &[
    "c c",
    "cc",
    "cedula",
    "dni",
    "documento",
    "identificacion",
    "nit",
    "rut",
    "tax id",
];

/// Words that make a header about something else: `Nombre de la cuenta`,
/// `Tipo de documento`, `Fecha documento`, `Código cliente`, `Ciudad cliente`.
const NOT_A_PERSON: &[&str] = &[
    "articulo",
    "banco",
    "ciudad",
    "codigo",
    "concepto",
    "cuenta",
    "departamento",
    "descripcion",
    "fecha",
    "item",
    "municipio",
    "pais",
    "producto",
    "servicio",
    "tipo",
    "total",
    "valor",
    "account",
    "city",
    "code",
    "date",
    "description",
    "product",
    "type",
];

/// Words of company names too common to hide on their own.
const COMMON_WORDS: &[&str] = &[
    "asociados",
    "banco",
    "colombia",
    "comercializadora",
    "compania",
    "construcciones",
    "consultores",
    "distribuidora",
    "general",
    "grupo",
    "ingenieria",
    "inversiones",
    "limitada",
    "ltda",
    "nacional",
    "oficina",
    "servicios",
    "soluciones",
    "sucursal",
    "transportes",
];

/// Terms to hide everywhere, from the values of person and document
/// columns: each value whole, and each of its words of four or more letters
/// (`María Gómez Restrepo` → also `María`, `Gómez`, `Restrepo`), so the
/// same people are hidden in free text like `Confirmar cuenta de María
/// Gómez`.
pub fn people_terms<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut terms = std::collections::BTreeSet::new();
    for value in values {
        let value = value.trim();
        if value.chars().count() < 4 {
            continue;
        }
        terms.insert(value.to_string());
        for word in value.split(|c: char| !c.is_alphanumeric()) {
            let folded: String = word.chars().map(fold_char).collect();
            let digits = word.chars().all(|c| c.is_ascii_digit());
            let long_enough = if digits {
                word.len() >= 6
            } else {
                word.chars().count() >= 4
            };
            if long_enough && !COMMON_WORDS.contains(&folded.as_str()) {
                terms.insert(word.to_string());
            }
        }
    }
    terms.into_iter().collect()
}

/// Rows searched for the header: reports often start with a title block.
const HEADER_SEARCH_ROWS: usize = 20;

/// Lower case, no accents, words separated by single spaces.
fn fold_header(header: &str) -> String {
    let folded: String = header
        .chars()
        .map(|c| {
            let c = fold_char(c);
            if c.is_alphanumeric() { c } else { ' ' }
        })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_word(header: &str, word: &str) -> bool {
    format!(" {header} ").contains(&format!(" {word} "))
}

/// The kind of a column from its header text.
pub fn column_kind(header: &str) -> Option<ColumnKind> {
    let h = fold_header(header);
    if h.is_empty() || NOT_A_PERSON.iter().any(|w| has_word(&h, w)) {
        return None;
    }
    if DOCUMENT_HEADERS.iter().any(|w| has_word(&h, w)) {
        Some(ColumnKind::Document)
    } else if PERSON_HEADERS.iter().any(|w| has_word(&h, w)) {
        Some(ColumnKind::Person)
    } else {
        None
    }
}

/// The header row of a table and the kind of each of its columns: the first
/// of the top rows with at least two text cells and one recognized header.
pub fn find_header<S: AsRef<str>>(rows: &[Vec<S>]) -> Option<(usize, Vec<Option<ColumnKind>>)> {
    rows.iter()
        .take(HEADER_SEARCH_ROWS)
        .enumerate()
        .find_map(|(i, row)| {
            let texts = row
                .iter()
                .filter(|c| {
                    let c = c.as_ref().trim();
                    !c.is_empty() && c.parse::<f64>().is_err()
                })
                .count();
            let kinds: Vec<_> = row.iter().map(|c| column_kind(c.as_ref())).collect();
            (texts >= 2 && kinds.iter().any(Option::is_some)).then_some((i, kinds))
        })
}

/// Everything one table pass found, for the summary and the vault.
#[derive(Debug, Default)]
pub struct Tally {
    pub categories: BTreeMap<Category, usize>,
    /// `(replacement, original)` of every hidden value.
    pub pairs: Vec<(String, String)>,
}

impl Tally {
    pub fn values(&self) -> usize {
        self.categories.values().sum()
    }

    fn add(&mut self, category: Category, replacement: &str, original: &str) {
        *self.categories.entry(category).or_default() += 1;
        self.pairs
            .push((replacement.to_string(), original.to_string()));
    }
}

impl Redactor {
    /// Redacts one cell. Cells under a person or document column are masked
    /// whole; any other cell goes through the regular engine.
    pub fn redact_cell(&self, value: &str, kind: Option<ColumnKind>, tally: &mut Tally) -> String {
        if value.trim().is_empty() {
            return value.to_string();
        }
        if kind.is_some() {
            let masked = self.mask_pii(value);
            if masked != value {
                tally.add(Category::Pii, &masked, value);
            }
            return masked;
        }
        let redaction = self.redact(value);
        for f in &redaction.findings {
            tally.add(f.category, &f.replacement, redaction.original(f));
        }
        redaction.output
    }

    /// Redacts a CSV file, keeping its delimiter, quoting and line endings.
    pub fn redact_csv(&self, text: &str, tally: &mut Tally) -> String {
        let delimiter = detect_delimiter(text);
        let rows = parse_csv(text, delimiter);
        let values: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.fields.iter().map(|f| f.value.as_str()).collect())
            .collect();
        let header = find_header(&values);

        // Names under person columns are hidden in every other cell too.
        let people = match &header {
            Some((h, kinds)) => people_terms(values.iter().skip(h + 1).flat_map(|row| {
                row.iter()
                    .enumerate()
                    .filter(|(j, _)| kinds.get(*j).copied().flatten().is_some())
                    .map(|(_, v)| *v)
            })),
            None => Vec::new(),
        };
        let r = self.with_people(people);

        let mut out = String::with_capacity(text.len());
        for (i, row) in rows.iter().enumerate() {
            for (j, field) in row.fields.iter().enumerate() {
                if j > 0 {
                    out.push(delimiter);
                }
                let kind = match &header {
                    Some((h, kinds)) if i > *h => kinds.get(j).copied().flatten(),
                    _ => None,
                };
                let value = r.redact_cell(&field.value, kind, tally);
                write_field(&mut out, &value, field.quoted, delimiter);
            }
            out.push_str(&row.ending);
        }
        out
    }
}

#[derive(Debug)]
struct Field {
    value: String,
    quoted: bool,
}

#[derive(Debug)]
struct Row {
    fields: Vec<Field>,
    /// `\n`, `\r\n` or empty for a last line without one.
    ending: String,
}

/// `;` is common where the decimal separator is a comma (Excel in Spanish).
fn detect_delimiter(text: &str) -> char {
    let first = text.lines().next().unwrap_or_default();
    [',', ';', '\t', '|']
        .into_iter()
        .max_by_key(|d| first.matches(*d).count())
        .filter(|d| first.contains(*d))
        .unwrap_or(',')
}

/// RFC 4180: quoted fields may hold delimiters, quotes (doubled) and newlines.
fn parse_csv(text: &str, delimiter: char) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut fields = Vec::new();
    let mut value = String::new();
    let (mut quoted, mut in_quotes) = (false, false);
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    value.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                value.push(c);
            }
            continue;
        }
        match c {
            '"' if value.is_empty() && !quoted => {
                quoted = true;
                in_quotes = true;
            }
            c if c == delimiter => {
                fields.push(Field {
                    value: std::mem::take(&mut value),
                    quoted,
                });
                quoted = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                fields.push(Field {
                    value: std::mem::take(&mut value),
                    quoted,
                });
                quoted = false;
                rows.push(Row {
                    fields: std::mem::take(&mut fields),
                    ending: "\r\n".into(),
                });
            }
            '\n' => {
                fields.push(Field {
                    value: std::mem::take(&mut value),
                    quoted,
                });
                quoted = false;
                rows.push(Row {
                    fields: std::mem::take(&mut fields),
                    ending: "\n".into(),
                });
            }
            c => value.push(c),
        }
    }
    if !value.is_empty() || quoted || !fields.is_empty() {
        fields.push(Field { value, quoted });
        rows.push(Row {
            fields,
            ending: String::new(),
        });
    }
    rows
}

fn write_field(out: &mut String, value: &str, quoted: bool, delimiter: char) {
    let needs = value.contains(delimiter) || value.contains(['"', '\n', '\r']);
    if quoted || needs {
        out.push('"');
        out.push_str(&value.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    #[test]
    fn classifies_headers() {
        use ColumnKind::*;
        let cases = [
            ("Nombre", Some(Person)),
            ("NOMBRE DEL CLIENTE", Some(Person)),
            ("Razón social", Some(Person)),
            ("Proveedor", Some(Person)),
            ("Teléfono", Some(Person)),
            ("NIT", Some(Document)),
            ("Cédula", Some(Document)),
            ("C.C.", Some(Document)),
            ("No. de identificación", Some(Document)),
            ("Nombre de la cuenta", None),
            ("Tipo de documento", None),
            ("Fecha documento", None),
            ("Código cliente", None),
            ("Valor total", None),
            ("Precio", None),
            ("Nominal", None),
        ];
        for (header, kind) in cases {
            assert_eq!(column_kind(header), kind, "{header}");
        }
    }

    #[test]
    fn finds_the_header_below_a_title_block() {
        let rows = vec![
            vec!["Informe de pagos", "", ""],
            vec!["Noviembre 2026", "", ""],
            vec!["", "", ""],
            vec!["Fecha", "Proveedor", "Valor"],
            vec!["2026-11-03", "Ferretería La Unión", "1250000"],
        ];
        let (row, kinds) = find_header(&rows).unwrap();
        assert_eq!(row, 3);
        assert_eq!(kinds, [None, Some(ColumnKind::Person), None]);
    }

    #[test]
    fn redacts_csv_columns_and_keeps_its_shape() {
        let r = Redactor::new(&Config {
            client: vec!["Globex".into()],
            ..Default::default()
        });
        let csv = "Fecha;Nombre;Cédula;Concepto;Valor\r\n\
                   2026-11-03;María Gómez;1020345678;\"Pago Globex; anticipo\";1.250.000\r\n\
                   2026-11-04;Pedro Ruiz;79123456;Honorarios NIT 900.123.456-8, ver María;800.000\r\n";
        let mut tally = Tally::default();
        let out = r.redact_csv(csv, &mut tally);
        assert!(
            !out.contains("María") && !out.contains("Gómez") && !out.contains("Pedro"),
            "{out}"
        );
        assert!(
            !out.contains("1020345678") && !out.contains("79123456"),
            "{out}"
        );
        assert!(!out.contains("900.123.456-8"), "{out}");
        assert!(out.contains("\"Pago [CLIENT]; anticipo\""), "{out}");
        assert!(
            out.contains("1.250.000") && out.contains("800.000"),
            "{out}"
        );
        assert!(
            out.starts_with("Fecha;Nombre;Cédula;Concepto;Valor\r\n"),
            "{out}"
        );
        assert_eq!(out.matches("\r\n").count(), 3);
        assert!(tally.values() >= 6);
    }

    #[test]
    fn people_terms_skip_short_and_common_words() {
        let terms = people_terms([
            "María Gómez Restrepo",
            "Distribuidora La Unión SAS",
            "1020345678",
            "Ana",
        ]);
        for t in [
            "María Gómez Restrepo",
            "María",
            "Gómez",
            "Restrepo",
            "Unión",
            "1020345678",
        ] {
            assert!(terms.contains(&t.to_string()), "{t}: {terms:?}");
        }
        for t in ["Distribuidora", "La", "SAS", "Ana"] {
            assert!(!terms.contains(&t.to_string()), "{t}: {terms:?}");
        }
    }

    #[test]
    fn parses_quoted_fields() {
        let rows = parse_csv("a,\"b,\"\"c\"\"\nd\",e\nf,g", ',');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].fields[1].value, "b,\"c\"\nd");
        assert_eq!(rows[1].ending, "");
        assert_eq!(detect_delimiter("a;b;c\n"), ';');
        assert_eq!(detect_delimiter("a\tb\n"), '\t');
        assert_eq!(detect_delimiter("solo\n"), ',');
    }
}
