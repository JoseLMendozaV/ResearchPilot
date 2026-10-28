import io
import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from unittest import mock

sys.path.insert(0, os.path.dirname(__file__))

from pdf_fixtures import scanned_page, scientific_paper  # noqa: E402

from researchpilot_worker import PROTOCOL_VERSION, grobid, heuristics  # noqa: E402
from researchpilot_worker.document import Page, canonical_section  # noqa: E402
from researchpilot_worker.extract import run_extract  # noqa: E402
from researchpilot_worker.layout import basic_text_page, order_lines, words_to_lines  # noqa: E402
from researchpilot_worker.ocr import tesseract_path  # noqa: E402
from researchpilot_worker.protocol import Emitter, WorkerError  # noqa: E402

try:
    import pdfplumber  # noqa: F401

    HAS_PDFPLUMBER = True
except ImportError:
    HAS_PDFPLUMBER = False


def run_worker(request: dict) -> list[dict]:
    completed = subprocess.run(
        [sys.executable, "-B", "-m", "researchpilot_worker", "extract"],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        timeout=120,
        cwd=os.path.join(os.path.dirname(__file__), ".."),
    )
    return [json.loads(line) for line in completed.stdout.splitlines() if line.strip()]


def by_field(result: dict) -> dict:
    grouped: dict = {}
    for candidate in result["candidates"]:
        grouped.setdefault(candidate["fieldKey"], []).append(candidate)
    return grouped


class FixtureMixin:
    @classmethod
    def setUpClass(cls) -> None:
        cls.directory = tempfile.TemporaryDirectory()
        cls.paper = os.path.join(cls.directory.name, "paper.pdf")
        scientific_paper(cls.paper)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.directory.cleanup()


@unittest.skipUnless(HAS_PDFPLUMBER, "pdfplumber no instalado")
class LayoutExtractionTest(FixtureMixin, unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        super().setUpClass()
        events = run_worker({"protocolVersion": PROTOCOL_VERSION, "pdfPath": cls.paper, "options": {"ocr": {"enabled": False}}})
        cls.events = events
        cls.result = events[-1]["payload"]

    def test_protocol_stream_is_hello_progress_result(self) -> None:
        types = [event["type"] for event in self.events]
        self.assertEqual(types[0], "hello")
        self.assertIn("progress", types)
        self.assertEqual(types[-1], "result")
        self.assertEqual(types.count("result"), 1)

    def test_every_quote_is_literal_page_text_with_page_and_bbox(self) -> None:
        pages = {page["pageNumber"]: page["text"] for page in self.result["pages"]}
        self.assertGreater(len(self.result["candidates"]), 10)
        for candidate in self.result["candidates"]:
            for evidence in candidate["evidence"]:
                self.assertIn(evidence["quote"], pages[evidence["page"]], candidate["fieldKey"])
                self.assertLessEqual(len(evidence["quote"]), heuristics.MAX_QUOTE)
                self.assertEqual(len(evidence["bbox"]), 4)

    def test_two_column_page_is_read_left_column_first(self) -> None:
        page2 = next(page for page in self.result["pages"] if page["pageNumber"] == 2)
        self.assertEqual(page2["columns"], 2)
        text = page2["text"]
        self.assertLess(text.index("cross-validation"), text.index("3 Results"))
        self.assertLess(text.index("3 Results"), text.index("F1-score of 0.91"))

    def test_sections_are_canonicalized_without_false_headings(self) -> None:
        titles = [(section["title"], section["canonical"]) for section in self.result["sections"]]
        self.assertIn(("2 Methods", "methods"), titles)
        self.assertIn(("References", "references"), titles)
        self.assertFalse(any("proposed method" in title for title, _ in titles))

    def test_expected_fields_are_proposed_with_evidence(self) -> None:
        fields = by_field(self.result)
        self.assertEqual(fields["bibliography.doi"][0]["value"], "10.1234/rp.2026.001")
        self.assertEqual(fields["bibliography.title"][0]["value"], "Traceable Evidence Extraction for Systematic Reviews")
        self.assertEqual(fields["bibliography.keywords"][0]["value"], ["systematic review", "provenance", "information extraction"])
        self.assertEqual([item["value"][:4] for item in fields["context.research_questions"]], ["RQ1:", "RQ2:"])
        sample = fields["design_methods.sample_size"][0]
        self.assertEqual(sample["value"], 120)
        self.assertEqual(len(sample["evidence"]), 2, "the same value cited twice keeps both evidences")
        metrics = {(item["value"]["metric"], item["value"]["value"]) for item in fields["evaluation_results.metrics"]}
        self.assertIn(("accuracy", 93.2), metrics)
        self.assertIn(("f1-score", 0.91), metrics)
        self.assertFalse(any(value == 2025 for _, value in metrics), "years are not metrics")
        self.assertEqual(fields["design_methods.software_versions"][0]["value"], "PyTorch 1.12")
        self.assertEqual(fields["ethics_reproducibility.repositories"][0]["value"], "https://github.com/example/researchpilot-fixtures")
        self.assertEqual(fields["evaluation_results.future_work"][0]["value"], "Future work will add OCR.")

    def test_not_found_is_reported_without_inventing_values(self) -> None:
        self.assertIn("ethics_reproducibility.ethics_approval", self.result["notFound"])
        self.assertNotIn("ethics_reproducibility.ethics_approval", by_field(self.result))
        self.assertNotIn("bibliography.doi", self.result["notFound"])

    def test_references_are_split_with_doi(self) -> None:
        references = self.result["references"]
        self.assertEqual([reference["ordinal"] for reference in references], [1, 2, 3])
        self.assertEqual(references[0]["doi"], "10.5555/jm.2020.1")
        self.assertIn("Technology 64 (2015)", references[2]["raw"])
        self.assertEqual(by_field(self.result)["bibliography.cited_references_count"][0]["value"], 3)

    def test_captions_become_table_entries(self) -> None:
        table = self.result["tablesFigures"][0]
        self.assertEqual((table["objectType"], table["label"], table["pageNumber"]), ("table", "Table 1", 2))

    def test_hash_is_reported_and_verified(self) -> None:
        events = run_worker({"protocolVersion": PROTOCOL_VERSION, "pdfPath": self.paper, "expectedSha256": "0" * 64})
        self.assertEqual(events[-1]["type"], "error")
        self.assertEqual(events[-1]["code"], "hash_mismatch")
        self.assertEqual(len(self.result["documentSha256"]), 64)


class WorkerErrorsTest(FixtureMixin, unittest.TestCase):
    def test_invalid_pdf_and_protocol_mismatch_are_typed_errors(self) -> None:
        fake = os.path.join(self.directory.name, "fake.pdf")
        with open(fake, "wb") as handle:
            handle.write(b"not a pdf")
        events = run_worker({"protocolVersion": PROTOCOL_VERSION, "pdfPath": fake})
        self.assertEqual((events[-1]["type"], events[-1]["code"]), ("error", "invalid_pdf"))
        events = run_worker({"protocolVersion": 999, "pdfPath": self.paper})
        self.assertEqual(events[-1]["code"], "protocol_mismatch")

    def test_basic_text_mode_without_pdfplumber(self) -> None:
        stream = io.StringIO()
        request = {
            "protocolVersion": PROTOCOL_VERSION,
            "pdfPath": self.paper,
            "fallbackPages": [
                {"pageNumber": 1, "text": "Abstract\nWe report an accuracy of 91.5% with 40 participants.\nKeywords: rules; evidence"},
                {"pageNumber": 2, "text": ""},
            ],
        }
        with mock.patch.dict(sys.modules, {"pdfplumber": None}):
            result = run_extract(request, Emitter(stream))
        self.assertEqual(result["engine"], "basic_text")
        self.assertIn("layout_unavailable", [warning["code"] for warning in result["warnings"]])
        self.assertEqual(result["pages"][1]["status"], "ocr_required")
        fields = by_field(result)
        self.assertEqual(fields["evaluation_results.metrics"][0]["value"]["value"], 91.5)
        self.assertIsNone(fields["evaluation_results.metrics"][0]["evidence"][0]["bbox"])
        self.assertNotIn("bibliography.title", result["notFound"], "title is not searched without font sizes")


@unittest.skipUnless(HAS_PDFPLUMBER and tesseract_path(), "OCR local no disponible")
class OcrTest(unittest.TestCase):
    def test_scanned_page_is_recognized_and_flagged_for_review(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "scan.pdf")
            if not scanned_page(path):
                self.skipTest("Pillow sin fuente escalable")
            disabled = run_worker({"protocolVersion": PROTOCOL_VERSION, "pdfPath": path, "options": {"ocr": {"enabled": False}}})[-1]["payload"]
            self.assertEqual(disabled["pages"][0]["status"], "ocr_required")
            self.assertIn("ocr_disabled", [warning["code"] for warning in disabled["warnings"]])

            result = run_worker({"protocolVersion": PROTOCOL_VERSION, "pdfPath": path, "options": {"ocr": {"enabled": True, "languages": "eng"}}})[-1]["payload"]
        page = result["pages"][0]
        self.assertEqual(page["textSource"], "ocr")
        self.assertIn("88.5%", page["text"])
        self.assertGreater(page["ocrConfidence"], 0.5)
        metric = by_field(result)["evaluation_results.metrics"][0]
        self.assertEqual(metric["origin"], "ocr")
        self.assertTrue(metric["needsReview"])


class RuleUnitTest(unittest.TestCase):
    def corpus(self, text: str) -> heuristics.Corpus:
        page = basic_text_page(1, text)
        return heuristics.Corpus([page])

    def test_vocabulary_is_case_sensitive_for_acronyms(self) -> None:
        corpus = self.corpus("We clip gradients and use the CLIP model with random forests.")
        names = [candidate.value for rule in heuristics.VOCABULARY_RULES for candidate in rule(corpus)]
        self.assertEqual(names.count("CLIP"), 1)
        self.assertIn("Random Forest", names)

    def test_canonical_sections_require_heading_shape(self) -> None:
        self.assertEqual(canonical_section("III. Materials and Methods"), "methods")
        self.assertEqual(canonical_section("5 Conclusions and Future Work"), "conclusion")
        self.assertIsNone(canonical_section("The proposed method achieved an"))

    def test_two_column_ordering(self) -> None:
        words = []
        for row in range(6):
            words.append({"text": f"left{row}", "x0": 50, "x1": 200, "top": 100 + row * 12, "bottom": 110 + row * 12, "size": 10})
            words.append({"text": f"right{row}", "x0": 320, "x1": 480, "top": 100 + row * 12, "bottom": 110 + row * 12, "size": 10})
        columns, ordered = order_lines(words_to_lines(words), 595)
        self.assertEqual(columns, 2)
        self.assertEqual([line.text for line, _ in ordered][:7], [f"left{row}" for row in range(6)] + ["right0"])

    def test_pages_without_blocks_need_ocr(self) -> None:
        self.assertIsInstance(basic_text_page(3, "   "), Page)
        self.assertEqual(basic_text_page(3, "   ").status, "ocr_required")


TEI = b"""<?xml version="1.0" encoding="UTF-8"?>
<TEI xmlns="http://www.tei-c.org/ns/1.0"><teiHeader><fileDesc>
<titleStmt><title level="a" type="main">Traceable Evidence Extraction for Systematic Reviews</title></titleStmt>
<sourceDesc><biblStruct><analytic>
<author><persName><forename>Ana</forename><surname>Researcher</surname></persName><idno type="ORCID">0000-0002-1825-0097</idno>
<affiliation><orgName>University of Evidence</orgName><address><country>Peru</country></address></affiliation></author>
</analytic><monogr><title level="j">Journal of Methods</title><imprint><date type="published" when="2026-01-15"/></imprint></monogr>
<idno type="DOI">10.1234/RP.2026.001</idno></biblStruct></sourceDesc></fileDesc>
<profileDesc><abstract><p>This paper studies how literal quotes can support every extracted value.</p></abstract>
<textClass><keywords><term>provenance</term></keywords></textClass></profileDesc></teiHeader>
<text><body><div><head>Introduction</head></div></body><back><listBibl>
<biblStruct><analytic><title>Evidence in reviews</title></analytic><idno type="DOI">10.5555/jm.2020.1</idno>
<note type="raw_reference">A. Author, Evidence in reviews, Journal of Methods, 2020.</note></biblStruct>
</listBibl></back></text></TEI>"""


class GrobidTest(FixtureMixin, unittest.TestCase):
    def test_parse_tei_header_and_references(self) -> None:
        parsed = grobid.parse_tei(TEI)
        self.assertEqual(parsed["doi"], "10.1234/rp.2026.001")
        self.assertEqual(parsed["authors"][0]["name"], "Ana Researcher")
        self.assertEqual(parsed["authors"][0]["countries"], ["Peru"])
        self.assertEqual(parsed["references"][0]["doi"], "10.5555/jm.2020.1")
        self.assertEqual(parsed["publicationDate"], "2026-01-15")

    def test_remote_grobid_is_blocked_without_authorization(self) -> None:
        with self.assertRaises(WorkerError) as context:
            grobid.check_url("https://grobid.example.org", allow_remote=False)
        self.assertEqual(context.exception.code, "grobid_remote_blocked")
        self.assertEqual(grobid.check_url("http://127.0.0.1:8070/", allow_remote=False), "http://127.0.0.1:8070")
        self.assertTrue(grobid.is_loopback("http://localhost:8070"))

    @unittest.skipUnless(HAS_PDFPLUMBER, "pdfplumber no instalado")
    def test_local_grobid_service_adds_located_candidates(self) -> None:
        received = {}

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b"true")

            def do_POST(self) -> None:  # noqa: N802
                body = self.rfile.read(int(self.headers["Content-Length"]))
                received["consolidation_disabled"] = b'name="consolidateHeader"\r\n\r\n0' in body
                self.send_response(200)
                self.send_header("Content-Type", "application/xml")
                self.end_headers()
                self.wfile.write(TEI)

            def log_message(self, *args: object) -> None:
                pass

        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            url = f"http://127.0.0.1:{server.server_port}"
            self.assertTrue(grobid.is_alive(url, allow_remote=False))
            result = run_extract(
                {"protocolVersion": PROTOCOL_VERSION, "pdfPath": self.paper, "options": {"grobid": {"enabled": True, "url": url}}},
                Emitter(io.StringIO()),
            )
        finally:
            server.shutdown()
            server.server_close()
        self.assertTrue(received["consolidation_disabled"])
        self.assertTrue(result["grobidUsed"])
        grobid_fields = {candidate["fieldKey"]: candidate for candidate in result["candidates"] if candidate["origin"] == "grobid"}
        self.assertEqual(grobid_fields["bibliography.authors"]["value"], ["Ana Researcher"])
        self.assertEqual(grobid_fields["bibliography.authors"]["evidence"][0]["page"], 1)
        self.assertTrue(grobid_fields["bibliography.venue"]["needsReview"], "venue is not in the PDF text, so it cannot be verified")
        self.assertEqual(result["references"][0]["doi"], "10.5555/jm.2020.1")


if __name__ == "__main__":
    unittest.main()
