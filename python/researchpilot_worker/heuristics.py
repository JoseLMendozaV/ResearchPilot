"""Deterministic rules that propose master-record values with literal evidence.

Every candidate carries at least one quote that is an exact substring of the
page text returned to Rust, plus the page number and, when the layout engine
ran, a bounding box. Rules never invent values: a field the rules searched
without a match is reported as ``not_found`` (the extractor did not find it),
which is different from ``not_reported`` (a human confirmed the paper does
not report it).
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Callable, Iterable, Optional, Pattern

from .document import HEADER_FOOTER, Block, Page

MAX_QUOTE = 300
SKIP_SECTIONS = {"references"}


@dataclass
class Candidate:
    field_key: str
    value: Any
    display_value: str
    value_type: str
    method: str
    evidence: list[dict[str, Any]] = field(default_factory=list)
    unit: Optional[str] = None
    origin: str = "rules"
    needs_review: bool = False
    notes: str = ""

    def to_json(self) -> dict[str, Any]:
        return {
            "fieldKey": self.field_key,
            "value": self.value,
            "displayValue": self.display_value,
            "valueType": self.value_type,
            "unit": self.unit,
            "method": self.method,
            "origin": self.origin,
            "needsReview": self.needs_review,
            "notes": self.notes,
            "evidence": self.evidence,
        }


class Corpus:
    """Blocks in reading order plus helpers to cut verifiable quotes."""

    def __init__(self, pages: list[Page]) -> None:
        self.pages = pages
        self.blocks = [block for page in pages for block in page.blocks if block.block_type != HEADER_FOOTER and block.text]
        self.ocr_pages = {page.page_number for page in pages if page.text_source == "ocr"}

    def body_blocks(self, sections: Optional[set[str]] = None, max_page: Optional[int] = None) -> Iterable[Block]:
        for block in self.blocks:
            if block.canonical_section in SKIP_SECTIONS or block.block_type == "reference":
                continue
            if max_page is not None and block.page_number > max_page:
                continue
            if sections is not None and block.canonical_section not in sections:
                continue
            yield block

    def evidence(self, block: Block, start: int, end: int) -> dict[str, Any]:
        start, end = clip(block.text, start, end)
        return {
            "quote": block.text[start:end],
            "page": block.page_number,
            "bbox": block.bbox_for_range(start, end),
            "section": block.section,
        }

    def candidate(self, field_key: str, value: Any, display: str, value_type: str, method: str, block: Block, start: int, end: int, unit: Optional[str] = None) -> Candidate:
        ocr = block.page_number in self.ocr_pages
        return Candidate(
            field_key=field_key,
            value=value,
            display_value=display,
            value_type=value_type,
            method=method,
            unit=unit,
            evidence=[self.evidence(block, start, end)],
            origin="ocr" if ocr else "rules",
            needs_review=ocr,
            notes="Texto obtenido por OCR; verifica contra la imagen de la página." if ocr else "",
        )


def clip(text: str, start: int, end: int, limit: int = MAX_QUOTE) -> tuple[int, int]:
    """Trim whitespace and cap the length on a word boundary, keeping an exact substring."""
    while start < end and text[start].isspace():
        start += 1
    while end > start and text[end - 1].isspace():
        end -= 1
    if end - start > limit:
        cut = text.rfind(" ", start, start + limit)
        end = cut if cut > start + limit // 2 else start + limit
    return start, end


_SENTENCE_END = re.compile(r"(?<=[.!?])\s+(?=[A-Z0-9\[(“\"¿¡])")


def sentence_bounds(text: str, position: int) -> tuple[int, int]:
    start = 0
    # Scan the whole text: an ``endpos`` would hide the lookahead at the boundary.
    for match in _SENTENCE_END.finditer(text):
        if match.end() > position:
            break
        start = match.end()
    following = _SENTENCE_END.search(text, position)
    end = following.start() if following else len(text)
    return start, end


def normalize_space(value: str) -> str:
    return " ".join(value.split())


# --- Individual rules ---------------------------------------------------------

DOI = re.compile(r"\b(10\.\d{4,9}/[^\s\"<>]+)", re.IGNORECASE)
ISSN = re.compile(r"\b(?:e-?)?ISSN[:\s]*([0-9]{4}-[0-9]{3}[0-9Xx])")
ISBN = re.compile(r"\bISBN(?:-1[03])?[:\s]*((?:97[89][\s-]?)?[0-9][0-9\s-]{8,14}[0-9Xx])")
URL = re.compile(r"(?:https?://|www\.)[^\s<>\"]+|\b(?:github\.com|gitlab\.com|zenodo\.org|figshare\.com|osf\.io|huggingface\.co|kaggle\.com|bitbucket\.org|datadryad\.org|codeocean\.com)/[^\s<>\"]+", re.IGNORECASE)
REPOSITORY_HOSTS = ("github.com", "gitlab.com", "zenodo.org", "figshare.com", "osf.io", "huggingface.co", "kaggle.com", "bitbucket.org", "datadryad.org", "codeocean.com", "doi.org/10.5281", "dataverse")


def clean_doi(value: str) -> str:
    return value.rstrip(".,;:)]}>'\"").lower()


def rule_doi(corpus: Corpus) -> list[Candidate]:
    found: dict[str, Candidate] = {}
    for block in corpus.body_blocks(max_page=2):
        for match in DOI.finditer(block.text):
            doi = clean_doi(match.group(1))
            if doi not in found and len(found) < 3:
                found[doi] = corpus.candidate("bibliography.doi", doi, doi, "text", "rules:doi-regex", block, match.start(1), match.start(1) + len(doi))
    return list(found.values())


def rule_identifiers(corpus: Corpus) -> list[Candidate]:
    candidates: list[Candidate] = []
    seen: set[tuple[str, str]] = set()
    last_page = max((page.page_number for page in corpus.pages), default=1)
    for block in corpus.body_blocks():
        if block.page_number > 2 and block.page_number != last_page:
            continue
        for key, pattern in (("bibliography.issn", ISSN), ("bibliography.isbn", ISBN)):
            for match in pattern.finditer(block.text):
                value = normalize_space(match.group(1))
                if (key, value) not in seen:
                    seen.add((key, value))
                    candidates.append(corpus.candidate(key, value, value, "text", f"rules:{key.split('.')[-1]}-regex", block, match.start(), match.end()))
    return candidates


def rule_title(corpus: Corpus, body_size: Optional[float]) -> list[Candidate]:
    if not corpus.pages or body_size is None:
        return []
    first = corpus.pages[0]
    if not first.height:
        return []
    options = [
        block
        for block in first.blocks
        if block.font_size
        and block.bbox
        and block.bbox[1] < first.height * 0.5
        and block.block_type in ("heading", "paragraph")
        and 3 <= len(block.text.split()) <= 40
        and block.canonical_section is None
    ]
    if not options:
        return []
    best = max(options, key=lambda block: (block.font_size or 0, -(block.bbox or [0, 0])[1]))
    if (best.font_size or 0) < body_size * 1.2:
        return []
    title = normalize_space(best.text)
    candidate = corpus.candidate("bibliography.title", title, title, "text", "rules:largest-font-first-page", best, 0, len(best.text))
    return [candidate]


ABSTRACT_INLINE = re.compile(r"^\s*(?:abstract|resumen|summary)\s*[—–\-:.]\s*", re.IGNORECASE)
KEYWORDS_INLINE = re.compile(r"^\s*(?:keywords|key\s+words|index\s+terms|palabras\s+clave)\s*[—–\-:.]?\s*", re.IGNORECASE)


def rule_abstract(corpus: Corpus) -> list[Candidate]:
    blocks = [block for block in corpus.body_blocks(max_page=3)]
    for index, block in enumerate(blocks):
        inline = ABSTRACT_INLINE.match(block.text) if block.block_type != "heading" else None
        if block.block_type == "heading" and block.canonical_section == "abstract" and len(block.text) < 40:
            body = []
            for following in blocks[index + 1 :]:
                if following.block_type in ("heading", "caption") or KEYWORDS_INLINE.match(following.text):
                    break
                body.append(following)
                if sum(len(item.text) for item in body) > 4000:
                    break
            if body:
                text = normalize_space(" ".join(item.text for item in body))
                return [corpus.candidate("bibliography.abstract", text, text, "text", "rules:abstract-heading", body[0], 0, len(body[0].text))]
        elif inline and len(block.text) - inline.end() > 80:
            text = normalize_space(block.text[inline.end() :])
            return [corpus.candidate("bibliography.abstract", text, text, "text", "rules:abstract-inline", block, inline.end(), len(block.text))]
    return []


def rule_keywords(corpus: Corpus) -> list[Candidate]:
    blocks = list(corpus.body_blocks(max_page=3))
    for index, block in enumerate(blocks):
        match = KEYWORDS_INLINE.match(block.text)
        target, start = None, 0
        if match and len(block.text) - match.end() > 3:
            target, start = block, match.end()
        elif block.block_type == "heading" and block.canonical_section == "keywords" and index + 1 < len(blocks):
            target, start = blocks[index + 1], 0
        if target is None:
            continue
        raw = target.text[start:]
        end = len(target.text)
        stop = re.search(r"(?:\n|\s{2,}|(?<=\.)\s+[A-Z][a-z]+\s)", raw)
        if stop and stop.start() > 3:
            raw = raw[: stop.start()]
            end = start + stop.start()
        separator = ";" if ";" in raw else ("·" if "·" in raw else ("•" if "•" in raw else ","))
        keywords = [normalize_space(item).strip(" .") for item in raw.split(separator)]
        keywords = [item for item in keywords if 1 < len(item) <= 80][:30]
        if keywords:
            return [corpus.candidate("bibliography.keywords", keywords, "; ".join(keywords), "list", "rules:keywords-line", target, start, end)]
    return []


def sentence_rule(
    field_key: str,
    pattern: Pattern[str],
    method: str,
    limit: int = 3,
    sections: Optional[set[str]] = None,
    preferred: Optional[set[str]] = None,
) -> Callable[[Corpus], list[Candidate]]:
    """Propose the sentences that match ``pattern`` (preferred sections first)."""

    def run(corpus: Corpus) -> list[Candidate]:
        blocks = list(corpus.body_blocks(sections=sections))
        if preferred:
            blocks.sort(key=lambda block: 0 if block.canonical_section in preferred else 1)
        candidates: list[Candidate] = []
        seen: set[str] = set()
        for block in blocks:
            if block.block_type in ("heading",):
                continue
            for match in pattern.finditer(block.text):
                start, end = sentence_bounds(block.text, match.start())
                sentence = normalize_space(block.text[start:end])
                if sentence in seen or len(sentence) < 12:
                    continue
                seen.add(sentence)
                candidates.append(corpus.candidate(field_key, sentence, sentence, "text", method, block, start, end))
                if len(candidates) >= limit:
                    return candidates
        return candidates

    return run


def section_rule(field_key: str, canonical: str, method: str, fallback: Optional[Callable[[Corpus], list[Candidate]]] = None) -> Callable[[Corpus], list[Candidate]]:
    """Use the first paragraph(s) of a dedicated section, else a sentence fallback."""

    def run(corpus: Corpus) -> list[Candidate]:
        blocks = [block for block in corpus.body_blocks(sections={canonical}) if block.block_type == "paragraph"]
        if blocks:
            text = normalize_space(" ".join(block.text for block in blocks[:3]))[:3000]
            return [corpus.candidate(field_key, text, text, "text", method, blocks[0], 0, len(blocks[0].text))]
        return fallback(corpus) if fallback else []

    return run


RQ = re.compile(r"\bRQ\s?(\d{1,2})\s*[:.)\-–—]\s*")


def rule_research_questions(corpus: Corpus) -> list[Candidate]:
    found: dict[str, Candidate] = {}
    for block in corpus.body_blocks():
        for match in RQ.finditer(block.text):
            number = match.group(1)
            if number in found:
                continue
            question_end = block.text.find("?", match.end())
            sentence_end = sentence_bounds(block.text, match.end())[1]
            end = question_end + 1 if 0 <= question_end < match.end() + 400 else sentence_end
            question = normalize_space(block.text[match.end() : end])
            if len(question) < 10:
                continue
            value = f"RQ{number}: {question}"
            found[number] = corpus.candidate("context.research_questions", value, value, "text", "rules:rq-label", block, match.start(), end)
            if len(found) >= 15:
                break
    return sorted(found.values(), key=lambda candidate: int(re.findall(r"\d+", candidate.display_value)[0]))


SAMPLE_SIZE = re.compile(
    r"\b[Nn]\s*=\s*(\d{1,3}(?:,\d{3})+|\d+)\b|\b(\d{1,3}(?:,\d{3})+|\d+)\s+(?:participants|subjects|patients|students|respondents|teachers|users|volunteers|children|adults|employees|interviewees|participantes|estudiantes|pacientes|docentes|encuestados)\b"
)


def rule_sample_size(corpus: Corpus) -> list[Candidate]:
    found: dict[int, Candidate] = {}
    for block in corpus.body_blocks():
        for match in SAMPLE_SIZE.finditer(block.text):
            number = int((match.group(1) or match.group(2)).replace(",", ""))
            if number <= 0:
                continue
            start, end = sentence_bounds(block.text, match.start())
            if number in found:
                # Repeated mentions become additional evidence for the same value.
                if len(found[number].evidence) < 3:
                    found[number].evidence.append(corpus.evidence(block, start, end))
                continue
            if len(found) < 5:
                found[number] = corpus.candidate("design_methods.sample_size", number, str(number), "number", "rules:sample-size", block, start, end)
    return list(found.values())


DATASET = re.compile(r"\b((?:[A-Z][\w.\-]*)(?:[ \-](?:[A-Z0-9][\w.\-]*)){0,3})\s+(?:dataset|data\s+set|corpus|benchmark)s?\b")
DATASET_STOP = {"The", "This", "These", "Our", "A", "An", "In", "Each", "Both", "All", "Public", "Large", "New", "Original", "Training", "Test", "Validation", "Real", "Synthetic", "Table", "Fig", "Figure"}


def rule_datasets(corpus: Corpus) -> list[Candidate]:
    found: dict[str, Candidate] = {}
    for block in corpus.body_blocks():
        for match in DATASET.finditer(block.text):
            name = match.group(1).strip(" .-")
            words = name.split()
            while words and words[0] in DATASET_STOP:
                words.pop(0)
            name = " ".join(words)
            if len(name) < 2 or not any(character.isupper() or character.isdigit() for character in name) or name.lower() in found:
                continue
            offset = match.group(1).find(name)
            start = match.start(1) + max(offset, 0)
            evidence_start, evidence_end = sentence_bounds(block.text, start)
            found[name.lower()] = corpus.candidate("design_methods.datasets", name, name, "text", "rules:named-dataset", block, evidence_start, evidence_end)
            if len(found) >= 10:
                return list(found.values())
    return list(found.values())


def vocabulary_rule(field_key: str, terms: dict[str, str], method: str, limit: int = 10, version_field: Optional[str] = None) -> Callable[[Corpus], list[Candidate]]:
    """Match a controlled vocabulary (``regex -> canonical name``)."""
    # Case-sensitive so acronyms (CLIP, AMOS, R) do not match ordinary words;
    # lower-case phrases opt in with an inline ``(?i:...)`` group.
    suffix = r"(?:\s*(?:version|ver\.|v)?\s*(\d+(?:\.\d+){1,3}))?" if version_field else ""
    compiled = [(re.compile(f"(?:{pattern}){suffix}"), name) for pattern, name in terms.items()]

    def run(corpus: Corpus) -> list[Candidate]:
        found: dict[str, Candidate] = {}
        versions: dict[str, Candidate] = {}
        for block in corpus.body_blocks():
            for pattern, name in compiled:
                for match in pattern.finditer(block.text):
                    start, end = sentence_bounds(block.text, match.start())
                    if name not in found and len(found) < limit:
                        found[name] = corpus.candidate(field_key, name, name, "text", method, block, start, end)
                    version = match.groups()[-1] if version_field and match.groups() else None
                    if version_field and version:
                        label = f"{name} {version}"
                        if label not in versions:
                            versions[label] = corpus.candidate(version_field, label, label, "text", method + "-version", block, start, end)
        return list(found.values()) + list(versions.values())

    return run


SOFTWARE = {
    r"\bPyTorch\b": "PyTorch",
    r"\bTensorFlow\b": "TensorFlow",
    r"\bKeras\b": "Keras",
    r"\bscikit-learn\b|\bsklearn\b": "scikit-learn",
    r"\bJAX\b": "JAX",
    r"\bOpenCV\b": "OpenCV",
    r"\bHugging\s?Face\b": "Hugging Face",
    r"\bMATLAB\b": "MATLAB",
    r"\bSPSS\b": "SPSS",
    r"\bStata\b": "Stata",
    r"\bNVivo\b": "NVivo",
    r"\bATLAS\.ti\b": "ATLAS.ti",
    r"\bMAXQDA\b": "MAXQDA",
    r"\bWeka\b": "Weka",
    r"\bSmartPLS\b": "SmartPLS",
    r"\bMplus\b": "Mplus",
    r"\bAMOS\b": "AMOS",
    r"\bR\s+(?=(?:version|software|package|statistical|\(version|v\d))": "R",
    r"\bPython\b(?=\s*(?:version\s*)?\d)": "Python",
    r"\bCUDA\b": "CUDA",
}

ARCHITECTURES = {
    r"\bResNet-?\d*\b": "ResNet",
    r"\bVGG-?\d*\b": "VGG",
    r"\bU-?Net\b": "U-Net",
    r"\bRoBERTa\b": "RoBERTa",
    r"\bBERT\b": "BERT",
    r"\bGPT-?\d(?:\.\d)?\b": "GPT",
    r"\bLLaMA\b|\bLlama\s?\d\b": "LLaMA",
    r"\bViT\b|\bVision Transformer\b": "ViT",
    r"\bYOLO(?:v\d+)?\b": "YOLO",
    r"\bFaster R-CNN\b": "Faster R-CNN",
    r"\bMask R-CNN\b": "Mask R-CNN",
    r"\bEfficientNet(?:-?B\d)?\b": "EfficientNet",
    r"\bDenseNet-?\d*\b": "DenseNet",
    r"\bMobileNet(?:V\d)?\b": "MobileNet",
    r"\bInception(?:-?v\d)?\b": "Inception",
    r"\bDeepLab(?:v3\+?)?\b": "DeepLab",
    r"\bSegNet\b": "SegNet",
    r"\bCLIP\b": "CLIP",
    r"\bDINO(?:v2)?\b": "DINO",
    r"\bSwin Transformer\b": "Swin Transformer",
}

ALGORITHMS = {
    r"\b(?i:support vector machines?)\b|\bSVM\b": "SVM",
    r"\b(?i:random forests?)\b": "Random Forest",
    r"\bXGBoost\b": "XGBoost",
    r"\bLightGBM\b": "LightGBM",
    r"\bk-means\b": "k-means",
    r"\b(?i:k-nearest neighbou?rs?)\b|\bkNN\b": "k-NN",
    r"\bLSTM\b": "LSTM",
    r"\bGRU\b": "GRU",
    r"\b(?i:naive bayes)\b": "Naive Bayes",
    r"\b(?i:decision trees?)\b": "Decision Tree",
    r"\b(?i:generative adversarial networks?)\b|\bGANs?\b": "GAN",
    r"\b(?i:autoencoders?)\b": "Autoencoder",
    r"\b(?i:graph neural networks?)\b|\bGNNs?\b": "GNN",
    r"\b(?i:convolutional neural networks?)\b|\bCNNs?\b": "CNN",
    r"\b(?i:transformers?)\b(?= (?:model|architecture|encoder|decoder))": "Transformer",
}

STATISTICS = {
    r"\b(?i:paired t-tests?)\b": "Paired t-test",
    r"\b(?<!paired )(?i:t-tests?)\b": "t-test",
    r"\bMANOVA\b": "MANOVA",
    r"\bANCOVA\b": "ANCOVA",
    r"\bANOVA\b": "ANOVA",
    r"\bWilcoxon\b": "Wilcoxon",
    r"\bMann[–-]Whitney\b": "Mann-Whitney U",
    r"\bKruskal[–-]Wallis\b": "Kruskal-Wallis",
    r"\bFriedman (?i:test)\b": "Friedman",
    r"\b(?i:chi-squared?)\b|χ2|χ²": "Chi-square",
    r"\bFisher'?s (?i:exact)\b": "Fisher's exact test",
    r"\bPearson(?:'s)? (?i:correlation)\b": "Pearson correlation",
    r"\bSpearman\b": "Spearman correlation",
    r"\bCohen'?s (?i:kappa)\b": "Cohen's kappa",
    r"\bKrippendorff\b": "Krippendorff's alpha",
    r"\bCronbach'?s (?i:alpha)\b": "Cronbach's alpha",
    r"\b(?i:logistic regression)\b": "Logistic regression",
    r"\b(?i:linear regression)\b": "Linear regression",
    r"\b(?i:mixed[- ]effects?)\b": "Mixed-effects model",
    r"\b(?i:structural equation model(?:l)?ing)\b|\bPLS-SEM\b": "Structural equation modeling",
    r"\b(?i:(?:exploratory|confirmatory) factor analysis)\b": "Factor analysis",
    r"\bMcNemar\b": "McNemar",
    r"\bBonferroni\b": "Bonferroni correction",
    r"\bTukey\b": "Tukey HSD",
}

HARDWARE = re.compile(
    r"\b(?:NVIDIA\s+)?(?:GeForce\s+)?(?:RTX|GTX|Tesla|Quadro|TITAN)\s*[A-Z]?\d{2,4}\w*(?:\s+Ti)?\b|\b(?:NVIDIA\s+)?(?:A100|V100|H100|P100|K80|T4|L4)\b(?:\s+GPUs?)?|\bTPU\s*v?\d?\b|\bApple\s+M\d(?:\s+(?:Pro|Max|Ultra))?\b|\bIntel\s+(?:Core|Xeon)\s+[\w\-]+"
)


def rule_hardware(corpus: Corpus) -> list[Candidate]:
    found: dict[str, Candidate] = {}
    for block in corpus.body_blocks():
        for match in HARDWARE.finditer(block.text):
            name = normalize_space(match.group(0))
            if name.lower() not in found:
                start, end = sentence_bounds(block.text, match.start())
                found[name.lower()] = corpus.candidate("design_methods.hardware", name, name, "text", "rules:hardware-vocabulary", block, start, end)
            if len(found) >= 5:
                return list(found.values())
    return list(found.values())


METRIC = re.compile(
    r"(?P<metric>\b(?:top-[15]\s+accuracy|accuracy|precision|recall|F1(?:[\s-]score)?|F-measure|ROC[\s-]AUC|AUROC|AUC|mAP(?:@[\d.:]+)?|mIoU|IoU|Dice(?:\s+(?:coefficient|score))?|RMSE|MAE|MSE|MAPE|BLEU(?:-\d)?|ROUGE(?:-[L\d])?|sensitivity|specificity|PSNR|SSIM|FID|perplexity|EER|Cohen'?s\s+kappa))\b"
    r"(?P<gap>[^.\d\n;]{0,40}?)(?P<value>\d+(?:\.\d+)?)\s*(?P<unit>%|dB)?",
    re.IGNORECASE,
)


def rule_metrics(corpus: Corpus) -> list[Candidate]:
    preferred = {"abstract", "results", "discussion", "conclusion"}
    blocks = sorted(corpus.body_blocks(), key=lambda block: 0 if block.canonical_section in preferred else 1)
    found: dict[tuple[str, float], Candidate] = {}
    for block in blocks:
        for match in METRIC.finditer(block.text):
            value = float(match.group("value"))
            unit = match.group("unit")
            if not unit and value.is_integer() and 1900 <= value <= 2100:
                continue
            if re.search(r"\b(?:table|fig\.?|figure|section|eq\.?|equation)\s*$", match.group("gap"), re.IGNORECASE):
                continue
            metric = normalize_space(match.group("metric")).lower()
            key = (metric, value)
            if key in found:
                continue
            display = f"{metric} = {match.group('value')}{(' ' + unit) if unit else ''}"
            candidate = corpus.candidate(
                "evaluation_results.metrics",
                {"metric": metric, "value": value, "unit": unit},
                display,
                "metric",
                "rules:metric-pattern",
                block,
                *sentence_bounds(block.text, match.start()),
                unit=unit,
            )
            candidate.notes = (candidate.notes + " " if candidate.notes else "") + "Confirma dataset, configuración y varianza antes de comparar métricas."
            found[key] = candidate
            if len(found) >= 25:
                return list(found.values())
    return list(found.values())


def rule_repositories(corpus: Corpus) -> list[Candidate]:
    found: dict[str, Candidate] = {}
    for block in corpus.body_blocks():
        for match in URL.finditer(block.text):
            url = match.group(0).rstrip(".,;:)]}>'\"")
            if not any(host in url.lower() for host in REPOSITORY_HOSTS) or url.lower() in found:
                continue
            start, end = sentence_bounds(block.text, match.start())
            found[url.lower()] = corpus.candidate("ethics_reproducibility.repositories", url, url, "url", "rules:repository-url", block, start, end)
            if len(found) >= 10:
                return list(found.values())
    return list(found.values())


def references_count(corpus: Corpus, references: list[dict[str, Any]]) -> list[Candidate]:
    if not references:
        return []
    first = references[0]
    quote = first["raw"][:200]
    return [
        Candidate(
            field_key="bibliography.cited_references_count",
            value=len(references),
            display_value=str(len(references)),
            value_type="number",
            method="rules:reference-count",
            evidence=[{"quote": quote, "page": first.get("page"), "bbox": first.get("bbox"), "section": "References"}],
            notes="Conteo de entradas detectadas en la sección de referencias; puede variar si el formato es irregular.",
        )
    ]


def _re(pattern: str) -> Pattern[str]:
    return re.compile(pattern, re.IGNORECASE)


SENTENCE_RULES: list[Callable[[Corpus], list[Candidate]]] = [
    sentence_rule("context.objectives", _re(r"\b(?:the\s+)?(?:main\s+|primary\s+|overall\s+|general\s+)?(?:aim|objective|goal|purpose)s?\s+of\s+(?:this|the\s+present|our)\s+(?:paper|study|work|research|article|review|survey)\s+(?:is|was|are|were)\b|\b(?:this|the\s+present)\s+(?:paper|study|work|article|research)\s+(?:aims|seeks|intends)\s+to\b|\bel\s+objetivo\s+(?:general\s+)?de\s+(?:este|esta|el\s+presente)\s+(?:trabajo|estudio|artículo|investigación)\b"), "rules:objective-phrase"),
    sentence_rule("context.contributions", _re(r"\b(?:main|key|major|principal)?\s*contributions?\s+(?:of\s+(?:this|our)\s+(?:paper|work|study)\s+)?(?:are|is|include|can\s+be\s+summarized)\b"), "rules:contribution-phrase"),
    sentence_rule("context.contributions", _re(r"\bwe\s+(?:propose|present|introduce)\b"), "rules:we-propose", limit=2, sections={"abstract", "introduction", None}),
    sentence_rule("context.hypotheses", _re(r"\bwe\s+hypothesi[sz]ed?\b|\bhypothes[ie]s\s+(?:is|are|was|were)\b|\bH\d\s*[:.]\s"), "rules:hypothesis-phrase", limit=5),
    sentence_rule("context.stated_gaps", _re(r"\bresearch\s+gap\b|\bgap\s+in\s+the\s+literature\b|\b(?:little|limited|scarce)\s+(?:research|attention|work|evidence)\b|\b(?:few|no)\s+(?:prior\s+)?(?:studies|works)\s+(?:have|has)\b"), "rules:gap-phrase"),
    sentence_rule("design_methods.data_split", _re(r"\b\d{1,2}\s*%\s*(?:of\s+the\s+\w+\s+)?(?:for|as)?\s*(?:training|train)\b|\b(?:train(?:ing)?|validation)\s*/\s*(?:validation|val|test(?:ing)?)\b|\b\d{2}\s*[:/]\s*\d{2}(?:\s*[:/]\s*\d{2})?\s+(?:split|ratio)\b"), "rules:data-split"),
    sentence_rule("design_methods.validation", _re(r"\b(?:\d+|five|ten)[\s-]fold\s+cross[\s-]validation\b|\bleave[\s-]one[\s-](?:subject[\s-])?out\b|\bcross[\s-]validation\b|\bhold[\s-]out\s+(?:set|validation)\b"), "rules:validation-scheme"),
    sentence_rule("design_methods.baselines", _re(r"\bbaselines?\b|\bstate[\s-]of[\s-]the[\s-]art\s+methods\b"), "rules:baseline-phrase", sections={"methods", "results", None}),
    sentence_rule("evaluation_results.significance", _re(r"\bp\s*(?:<|>|=|≤|≥)\s*0?\.\d+|\bstatistically\s+significant\b"), "rules:p-value", limit=5),
    sentence_rule("evaluation_results.comparisons", _re(r"\boutperform(?:s|ed|ing)?\b|\bcompared\s+(?:with|to|against)\b"), "rules:comparison-phrase", preferred={"results", "abstract", "conclusion"}),
    sentence_rule("evaluation_results.ablation", _re(r"\bablation\b"), "rules:ablation"),
    sentence_rule("evaluation_results.negative_findings", _re(r"\b(?:did|does)\s+not\s+(?:improve|outperform|find|show|yield)\b|\bfailed\s+to\s+(?:improve|outperform|find|show)\b|\bno\s+(?:statistically\s+)?significant\s+(?:difference|effect|improvement|association)\b"), "rules:negative-finding"),
    section_rule("evaluation_results.conclusions", "conclusion", "rules:conclusion-section"),
    section_rule("evaluation_results.limitations", "limitations", "rules:limitations-section", sentence_rule("evaluation_results.limitations", _re(r"\blimitations?\s+(?:of\s+(?:this|our|the\s+present)\s+(?:study|work|approach|research)|include|are)\b|\bthis\s+study\s+has\s+(?:several\s+|some\s+)?limitations\b"), "rules:limitation-phrase")),
    section_rule("evaluation_results.future_work", "future_work", "rules:future-work-section", sentence_rule("evaluation_results.future_work", _re(r"\bfuture\s+(?:work|research|studies|directions)\b|\btrabajo\s+futuro\b"), "rules:future-work-phrase", preferred={"conclusion", "discussion"})),
    section_rule("evaluation_results.threats_to_validity", "threats_to_validity", "rules:threats-section", sentence_rule("evaluation_results.threats_to_validity", _re(r"\bthreats?\s+to\s+(?:the\s+)?(?:internal\s+|external\s+|construct\s+|conclusion\s+)?validity\b"), "rules:threats-phrase")),
    section_rule("ethics_reproducibility.funding", "funding", "rules:funding-section", sentence_rule("ethics_reproducibility.funding", _re(r"\b(?:funded|supported|sponsored)\s+(?:in\s+part\s+)?by\b|\bgrant\s+(?:no\.?|number|agreement)\b|\bfinanciad[oa]\s+por\b"), "rules:funding-phrase", preferred={"acknowledgments", "funding"})),
    section_rule("ethics_reproducibility.conflicts_of_interest", "conflicts", "rules:conflicts-section", sentence_rule("ethics_reproducibility.conflicts_of_interest", _re(r"\bconflicts?\s+of\s+interests?\b|\bcompeting\s+(?:financial\s+)?interests?\b"), "rules:conflicts-phrase", limit=2)),
    section_rule("ethics_reproducibility.ethics_approval", "ethics", "rules:ethics-section", sentence_rule("ethics_reproducibility.ethics_approval", _re(r"\bethic(?:s|al)\s+(?:committee|approval|review\s+board|board)\b|\binstitutional\s+review\s+board\b|\bIRB\b|\binformed\s+consent\b|\bcomité\s+de\s+ética\b"), "rules:ethics-phrase")),
    section_rule("ethics_reproducibility.data_availability", "data_availability", "rules:data-availability-section", sentence_rule("ethics_reproducibility.data_availability", _re(r"\b(?:data|datasets?)\s+(?:(?:are|is|will\s+be)\s+(?:made\s+)?(?:publicly\s+|openly\s+|freely\s+)?(?:available|accessible))\b|\bavailable\s+(?:up)?on\s+(?:reasonable\s+)?request\b"), "rules:data-availability-phrase")),
    sentence_rule("ethics_reproducibility.code_availability", _re(r"\b(?:source\s+)?code\s+(?:is|are|will\s+be|has\s+been)\s+(?:made\s+)?(?:publicly\s+|openly\s+|freely\s+)?(?:available|released|accessible)\b|\bimplementation\s+(?:is|will\s+be)\s+(?:publicly\s+)?available\b"), "rules:code-availability-phrase"),
    sentence_rule("ethics_reproducibility.supplementary_material", _re(r"\bsupplementary\s+(?:material|materials|information|file|files|data)\b|\bonline\s+appendix\b"), "rules:supplementary-phrase", limit=2),
]

VOCABULARY_RULES: list[Callable[[Corpus], list[Candidate]]] = [
    vocabulary_rule("design_methods.software", SOFTWARE, "rules:software-vocabulary", version_field="design_methods.software_versions"),
    vocabulary_rule("design_methods.architectures", ARCHITECTURES, "rules:architecture-vocabulary", limit=15),
    vocabulary_rule("design_methods.algorithms", ALGORITHMS, "rules:algorithm-vocabulary", limit=15),
    vocabulary_rule("design_methods.statistical_methods", STATISTICS, "rules:statistics-vocabulary"),
]

# Fields these rules look for; a field without candidates is reported as not_found.
SEARCHED_FIELDS = [
    "bibliography.doi",
    "bibliography.abstract",
    "bibliography.keywords",
    "bibliography.cited_references_count",
    "context.research_questions",
    "context.objectives",
    "context.contributions",
    "context.hypotheses",
    "context.stated_gaps",
    "design_methods.sample_size",
    "design_methods.datasets",
    "design_methods.data_split",
    "design_methods.validation",
    "design_methods.baselines",
    "design_methods.hardware",
    "design_methods.software",
    "design_methods.architectures",
    "design_methods.algorithms",
    "design_methods.statistical_methods",
    "evaluation_results.metrics",
    "evaluation_results.significance",
    "evaluation_results.comparisons",
    "evaluation_results.ablation",
    "evaluation_results.negative_findings",
    "evaluation_results.conclusions",
    "evaluation_results.limitations",
    "evaluation_results.future_work",
    "evaluation_results.threats_to_validity",
    "ethics_reproducibility.funding",
    "ethics_reproducibility.conflicts_of_interest",
    "ethics_reproducibility.ethics_approval",
    "ethics_reproducibility.data_availability",
    "ethics_reproducibility.code_availability",
    "ethics_reproducibility.repositories",
    "ethics_reproducibility.supplementary_material",
]


def run_rules(pages: list[Page], references: list[dict[str, Any]], body_size: Optional[float]) -> tuple[list[Candidate], list[str]]:
    corpus = Corpus(pages)
    candidates: list[Candidate] = []
    candidates += rule_title(corpus, body_size)
    candidates += rule_doi(corpus)
    candidates += rule_identifiers(corpus)
    candidates += rule_abstract(corpus)
    candidates += rule_keywords(corpus)
    candidates += rule_research_questions(corpus)
    candidates += rule_sample_size(corpus)
    candidates += rule_datasets(corpus)
    candidates += rule_hardware(corpus)
    candidates += rule_metrics(corpus)
    candidates += rule_repositories(corpus)
    candidates += references_count(corpus, references)
    for rule in SENTENCE_RULES + VOCABULARY_RULES:
        candidates += rule(corpus)

    searched = list(SEARCHED_FIELDS)
    if body_size is not None:
        searched.append("bibliography.title")
    present = {candidate.field_key for candidate in candidates}
    not_found = [key for key in searched if key not in present]
    return candidates, not_found
