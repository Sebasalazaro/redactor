"""Builds tests/xlsx/report.xlsx: a fictional accountant's report.

Every name, NIT and number in it is made up. Run with openpyxl installed:
    python scripts/fixtures/make-report-xlsx.py
"""

import sys
from openpyxl import Workbook
from openpyxl.comments import Comment
from openpyxl.styles import Font

out = sys.argv[1] if len(sys.argv) > 1 else "crates/redactor-core/tests/xlsx/report.xlsx"
wb = Workbook()
ws = wb.active
ws.title = "Pagos Globex"
ws["A1"] = "Informe de pagos a proveedores"
ws["A1"].font = Font(bold=True, size=14)
ws["A2"] = "Globex Bank SAS - noviembre 2026"
ws.append([])
ws.append(["Fecha", "Proveedor", "NIT", "Cédula", "Concepto", "Valor"])
rows = [
    ("2026-11-03", "Ferretería La Unión", "900.123.456-8", None, "Materiales oficina Globex", 1250000),
    ("2026-11-04", "María Gómez Restrepo", None, 1020345678, "Honorarios contables", 3400000),
    ("2026-11-05", "Pedro Ruiz", None, 79123456, "Transporte, ver NIT 900.123.456-8", 180000.5),
]
for r in rows:
    ws.append(list(r))
ws["F8"] = "=SUM(F5:F7)"
ws["B6"].comment = Comment("Confirmar cuenta de María Gómez", "Ana Contadora")

res = wb.create_sheet("Resumen")
res.append(["Código", "Nombre de la cuenta", "Saldo"])
res.append(["110505", "Caja general", 5200000])
res.append(["233595", "Otras cuentas por pagar", 1800000])

wb.properties.creator = "Ana Contadora"
wb.properties.lastModifiedBy = "Ana Contadora"
wb.properties.title = "Pagos Globex noviembre"
wb.save(out)
print("wrote", out)
