"""Regenera el PDF de la biblioteca sintética con ReportLab.

La salida se versiona como fixture. `invariant=1` evita fechas y IDs variables.
"""

from pathlib import Path

from reportlab.pdfgen import canvas


OUTPUT = (
    Path(__file__).resolve().parents[1]
    / "tests"
    / "fixtures"
    / "producto-windows"
    / "01 estudios"
    / "constancia-matricula.pdf"
)


def main() -> None:
    pdf = canvas.Canvas(str(OUTPUT), pagesize=(595, 842), invariant=1)
    pdf.setTitle("Constancia de matrícula - ejemplo ficticio")
    pdf.setAuthor("Universidad Imaginaria - fixture de Brain")
    pdf.setFont("Helvetica-Bold", 16)
    pdf.drawString(64, 764, "Constancia de matrícula")
    pdf.setFont("Helvetica", 11)
    for y, line in [
        (721, "Documento de prueba de la Universidad Imaginaria."),
        (696, "Lidia Robles participa en el laboratorio de redes del curso 2026."),
        (671, "El justificante corresponde al expediente ficticio AURORA."),
        (646, "La matrícula se confirmó el 2 de octubre de 2026."),
        (607, "No acredita estudios ni identidad de una persona real."),
    ]:
        pdf.drawString(64, y, line)
    pdf.setStrokeColorRGB(0.38, 0.48, 0.67)
    pdf.line(64, 748, 530, 748)
    pdf.setFont("Helvetica-Oblique", 9)
    pdf.drawString(64, 88, "Ejemplo sintético para pruebas locales de Orchestrator's Brain")
    pdf.showPage()
    pdf.save()


if __name__ == "__main__":
    main()
