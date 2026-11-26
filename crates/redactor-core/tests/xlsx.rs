//! Excel workbooks. `tests/xlsx/report.xlsx` is a fictional accountant's
//! report built by `scripts/fixtures/make-report-xlsx.py`.
#![cfg(feature = "xlsx")]

use std::io::{Cursor, Read};

use redactor_core::table::Tally;
use redactor_core::{Config, Redactor};

const REPORT: &[u8] = include_bytes!("xlsx/report.xlsx");

fn redactor() -> Redactor {
    Redactor::new(&Config {
        client: vec!["Globex Bank".into(), "Globex".into()],
        ..Default::default()
    })
}

/// Every XML part of a workbook, concatenated.
fn all_xml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut all = String::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        all.push_str(&text);
    }
    all
}

const SECRETS: &[&str] = &[
    "María",
    "Gómez",
    "Restrepo",
    "Pedro",
    "Ruiz",
    "Ferretería",
    "Unión",
    "1020345678",
    "79123456",
    "900.123.456-8",
    "Globex",
    "Ana Contadora",
];

#[test]
fn hides_names_documents_and_authors_everywhere() {
    let original = all_xml(REPORT);
    for secret in SECRETS {
        assert!(original.contains(secret), "fixture lacks {secret}");
    }

    let mut tally = Tally::default();
    let out = redactor().redact_xlsx(REPORT, &mut tally).unwrap();
    let xml = all_xml(&out.bytes);
    for secret in SECRETS {
        assert!(!xml.contains(secret), "{secret} left in the workbook");
        assert!(
            !out.text.contains(secret),
            "{secret} left in the text:\n{}",
            out.text
        );
    }
    assert!(tally.values() >= 10, "{:?}", tally.categories);

    // To open the result by hand: REDACTOR_XLSX_OUT=out.xlsx cargo test ...
    if let Some(path) = std::env::var_os("REDACTOR_XLSX_OUT") {
        std::fs::write(&path, &out.bytes).unwrap();
        std::fs::write(std::path::Path::new(&path).with_extension("md"), &out.text).unwrap();
    }
}

#[test]
fn keeps_numbers_formulas_and_other_columns() {
    let out = redactor()
        .redact_xlsx(REPORT, &mut Tally::default())
        .unwrap();
    let xml = all_xml(&out.bytes);
    for kept in [
        "1250000",
        "3400000",
        "180000.5",
        "SUM(F5:F7)",
        "Caja general",
        "Nombre de la cuenta",
        "110505",
    ] {
        assert!(xml.contains(kept), "{kept} missing from the workbook");
    }
    // A sheet named after the client gets a clean name.
    assert!(xml.contains("name=\"Pagos CLIENT\""), "sheet not renamed");
    // The text version has a table per sheet, with redacted sheet names.
    assert!(out.text.contains("## Pagos [CLIENT]"), "{}", out.text);
    assert!(out.text.contains("## Resumen"));
    assert!(
        out.text
            .contains("| Fecha | Proveedor | NIT | Cédula | Concepto | Valor |"),
        "{}",
        out.text
    );
    assert!(out.text.contains("Caja general"));
}

#[test]
fn refuses_files_that_are_not_workbooks() {
    let r = redactor();
    assert!(r.redact_xlsx(b"plain text", &mut Tally::default()).is_err());
}

/// Any workbook from disk, for checking real files by hand (they are never
/// committed): REDACTOR_XLSX_IN=in.xlsx REDACTOR_XLSX_OUT=out.xlsx
/// cargo test -p redactor-core --features xlsx --test xlsx -- --ignored
#[test]
#[ignore]
fn redacts_a_workbook_from_disk() {
    let input = std::fs::read(std::env::var_os("REDACTOR_XLSX_IN").unwrap()).unwrap();
    let mut tally = Tally::default();
    let out = redactor().redact_xlsx(&input, &mut tally).unwrap();
    let path = std::env::var_os("REDACTOR_XLSX_OUT").unwrap();
    std::fs::write(&path, &out.bytes).unwrap();
    std::fs::write(std::path::Path::new(&path).with_extension("md"), &out.text).unwrap();
    println!("{} values hidden: {:?}", tally.values(), tally.categories);
}
