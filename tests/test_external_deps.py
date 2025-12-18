"""Tests for the external dependency analysis module."""

from pathlib import Path

from moss.external_deps import (
    Dependency,
    DependencyAnalysisResult,
    ExternalDependencyAnalyzer,
    ResolvedDependency,
    Vulnerability,
    create_external_dependency_analyzer,
)

# =============================================================================
# Dependency Tests
# =============================================================================


class TestDependency:
    def test_create_dependency(self):
        dep = Dependency(name="requests", version_spec=">=2.0")
        assert dep.name == "requests"
        assert dep.version_spec == ">=2.0"
        assert dep.extras == []
        assert not dep.is_dev
        assert not dep.is_optional

    def test_normalized_name(self):
        dep = Dependency(name="My_Package.Name")
        assert dep.normalized_name == "my-package-name"

    def test_to_dict(self):
        dep = Dependency(
            name="pytest",
            version_spec=">=8.0",
            extras=["dev"],
            is_dev=True,
        )
        d = dep.to_dict()
        assert d["name"] == "pytest"
        assert d["version_spec"] == ">=8.0"
        assert d["extras"] == ["dev"]
        assert d["is_dev"] is True


# =============================================================================
# ResolvedDependency Tests
# =============================================================================


class TestResolvedDependency:
    def test_create_resolved(self):
        dep = ResolvedDependency(name="requests", version="2.31.0")
        assert dep.name == "requests"
        assert dep.version == "2.31.0"
        assert dep.is_direct is True

    def test_weight_no_deps(self):
        dep = ResolvedDependency(name="simple", version="1.0")
        assert dep.weight == 1

    def test_weight_with_deps(self):
        dep = ResolvedDependency(
            name="requests",
            version="2.31.0",
            dependencies=[
                ResolvedDependency(name="urllib3", version="2.0"),
                ResolvedDependency(name="certifi", version="2023.0"),
            ],
        )
        assert dep.weight == 3  # self + 2 deps

    def test_weight_nested(self):
        dep = ResolvedDependency(
            name="a",
            version="1.0",
            dependencies=[
                ResolvedDependency(
                    name="b",
                    version="1.0",
                    dependencies=[
                        ResolvedDependency(name="c", version="1.0"),
                    ],
                ),
            ],
        )
        assert dep.weight == 3  # a + b + c

    def test_to_dict(self):
        dep = ResolvedDependency(name="requests", version="2.31.0")
        d = dep.to_dict()
        assert d["name"] == "requests"
        assert d["version"] == "2.31.0"
        assert d["weight"] == 1
        assert d["is_direct"] is True


# =============================================================================
# Vulnerability Tests
# =============================================================================


class TestVulnerability:
    def test_create_vulnerability(self):
        vuln = Vulnerability(
            id="CVE-2023-1234",
            package="requests",
            severity="HIGH",
            summary="SQL injection vulnerability",
        )
        assert vuln.id == "CVE-2023-1234"
        assert vuln.package == "requests"
        assert vuln.severity == "HIGH"
        assert vuln.summary == "SQL injection vulnerability"
        assert vuln.affected_versions == ""
        assert vuln.fixed_version == ""
        assert vuln.url == ""

    def test_vulnerability_with_all_fields(self):
        vuln = Vulnerability(
            id="GHSA-abcd-1234",
            package="flask",
            severity="CRITICAL",
            summary="Remote code execution",
            affected_versions="<2.0",
            fixed_version="2.0.1",
            url="https://example.com/advisory",
        )
        assert vuln.fixed_version == "2.0.1"
        assert vuln.url == "https://example.com/advisory"

    def test_vulnerability_to_dict(self):
        vuln = Vulnerability(
            id="CVE-2023-5678",
            package="django",
            severity="MEDIUM",
            summary="XSS vulnerability",
            fixed_version="4.0",
        )
        d = vuln.to_dict()
        assert d["id"] == "CVE-2023-5678"
        assert d["package"] == "django"
        assert d["severity"] == "MEDIUM"
        assert d["summary"] == "XSS vulnerability"
        assert d["fixed_version"] == "4.0"


# =============================================================================
# DependencyAnalysisResult Tests
# =============================================================================


class TestDependencyAnalysisResult:
    def test_empty_result(self):
        result = DependencyAnalysisResult()
        assert result.total_direct == 0
        assert result.total_dev == 0
        assert result.total_optional == 0
        assert result.total_transitive == 0

    def test_totals(self):
        result = DependencyAnalysisResult(
            dependencies=[
                Dependency(name="a"),
                Dependency(name="b"),
            ],
            dev_dependencies=[
                Dependency(name="c", is_dev=True),
            ],
            optional_dependencies={
                "docs": [Dependency(name="d"), Dependency(name="e")],
            },
        )
        assert result.total_direct == 2
        assert result.total_dev == 1
        assert result.total_optional == 2

    def test_total_transitive(self):
        result = DependencyAnalysisResult(
            resolved_tree=[
                ResolvedDependency(
                    name="a",
                    version="1.0",
                    dependencies=[
                        ResolvedDependency(name="b", version="1.0"),
                        ResolvedDependency(name="c", version="1.0"),
                    ],
                ),
            ]
        )
        # Weight is 3 (a + b + c), minus 1 direct = 2 transitive
        assert result.total_transitive == 2

    def test_heaviest_dependencies(self):
        result = DependencyAnalysisResult(
            resolved_tree=[
                ResolvedDependency(name="light", version="1.0"),
                ResolvedDependency(
                    name="heavy",
                    version="1.0",
                    dependencies=[
                        ResolvedDependency(name="x", version="1.0"),
                        ResolvedDependency(name="y", version="1.0"),
                    ],
                ),
            ]
        )
        heaviest = result.heaviest_dependencies
        assert heaviest[0].name == "heavy"
        assert heaviest[1].name == "light"

    def test_to_dict(self):
        result = DependencyAnalysisResult(
            sources=["pyproject.toml"],
            dependencies=[Dependency(name="a")],
        )
        d = result.to_dict()
        assert d["stats"]["direct"] == 1
        assert d["sources"] == ["pyproject.toml"]
        assert len(d["dependencies"]) == 1

    def test_to_markdown(self):
        result = DependencyAnalysisResult(
            sources=["pyproject.toml"],
            dependencies=[
                Dependency(name="requests", version_spec=">=2.0"),
            ],
            dev_dependencies=[
                Dependency(name="pytest", version_spec=">=8.0", is_dev=True),
            ],
        )
        md = result.to_markdown()
        assert "External Dependency Analysis" in md
        assert "requests" in md
        assert "pytest" in md
        assert ">=2.0" in md

    def test_get_heavy_dependencies(self):
        result = DependencyAnalysisResult(
            resolved_tree=[
                ResolvedDependency(name="light", version="1.0"),  # weight 1
                ResolvedDependency(
                    name="heavy",
                    version="1.0",
                    dependencies=[
                        ResolvedDependency(name="a", version="1.0"),
                        ResolvedDependency(name="b", version="1.0"),
                        ResolvedDependency(name="c", version="1.0"),
                    ],
                ),  # weight 4
            ]
        )
        heavy = result.get_heavy_dependencies(threshold=3)
        assert len(heavy) == 1
        assert heavy[0].name == "heavy"

    def test_get_heavy_dependencies_none(self):
        result = DependencyAnalysisResult(
            resolved_tree=[
                ResolvedDependency(name="light", version="1.0"),
            ]
        )
        heavy = result.get_heavy_dependencies(threshold=5)
        assert len(heavy) == 0

    def test_to_markdown_with_heavy_warning(self):
        result = DependencyAnalysisResult(
            sources=["pyproject.toml"],
            resolved_tree=[
                ResolvedDependency(
                    name="bloated",
                    version="1.0",
                    dependencies=[
                        ResolvedDependency(name=f"dep{i}", version="1.0") for i in range(10)
                    ],
                ),
            ],
        )
        md = result.to_markdown(weight_threshold=5)
        assert "Heavy Dependencies Warning" in md
        assert "bloated" in md

    def test_to_dict_with_heavy(self):
        result = DependencyAnalysisResult(
            resolved_tree=[
                ResolvedDependency(
                    name="heavy",
                    version="1.0",
                    dependencies=[
                        ResolvedDependency(name="a", version="1.0"),
                        ResolvedDependency(name="b", version="1.0"),
                    ],
                ),
            ]
        )
        d = result.to_dict(weight_threshold=2)
        assert d["stats"]["heavy_count"] == 1
        assert len(d["heavy_dependencies"]) == 1

    def test_has_vulnerabilities_false(self):
        result = DependencyAnalysisResult()
        assert not result.has_vulnerabilities

    def test_has_vulnerabilities_true(self):
        result = DependencyAnalysisResult(
            vulnerabilities=[
                Vulnerability(id="CVE-1", package="pkg", severity="LOW", summary="test"),
            ]
        )
        assert result.has_vulnerabilities

    def test_critical_vulns(self):
        result = DependencyAnalysisResult(
            vulnerabilities=[
                Vulnerability(id="CVE-1", package="a", severity="CRITICAL", summary="crit"),
                Vulnerability(id="CVE-2", package="b", severity="HIGH", summary="high"),
                Vulnerability(id="CVE-3", package="c", severity="CRITICAL", summary="crit2"),
            ]
        )
        assert len(result.critical_vulns) == 2
        assert all(v.severity == "CRITICAL" for v in result.critical_vulns)

    def test_high_vulns(self):
        result = DependencyAnalysisResult(
            vulnerabilities=[
                Vulnerability(id="CVE-1", package="a", severity="CRITICAL", summary="crit"),
                Vulnerability(id="CVE-2", package="b", severity="HIGH", summary="high"),
                Vulnerability(id="CVE-3", package="c", severity="MEDIUM", summary="med"),
            ]
        )
        assert len(result.high_vulns) == 1
        assert result.high_vulns[0].id == "CVE-2"

    def test_to_dict_with_vulnerabilities(self):
        result = DependencyAnalysisResult(
            vulnerabilities=[
                Vulnerability(id="CVE-1", package="a", severity="CRITICAL", summary="test"),
                Vulnerability(id="CVE-2", package="b", severity="HIGH", summary="test2"),
            ]
        )
        d = result.to_dict()
        assert d["stats"]["vulnerabilities"] == 2
        assert d["stats"]["critical_vulns"] == 1
        assert d["stats"]["high_vulns"] == 1
        assert len(d["vulnerabilities"]) == 2

    def test_to_markdown_with_vulnerabilities(self):
        result = DependencyAnalysisResult(
            sources=["pyproject.toml"],
            vulnerabilities=[
                Vulnerability(
                    id="CVE-2023-1234",
                    package="requests",
                    severity="CRITICAL",
                    summary="Remote code execution vulnerability",
                ),
                Vulnerability(
                    id="CVE-2023-5678",
                    package="flask",
                    severity="HIGH",
                    summary="SQL injection",
                ),
            ],
        )
        md = result.to_markdown()
        assert "Security Vulnerabilities" in md
        assert "CRITICAL" in md
        assert "CVE-2023-1234" in md
        assert "requests" in md


# =============================================================================
# ExternalDependencyAnalyzer Tests
# =============================================================================


class TestExternalDependencyAnalyzer:
    def test_create_analyzer(self, tmp_path: Path):
        analyzer = ExternalDependencyAnalyzer(tmp_path)
        assert analyzer.root == tmp_path

    def test_parse_pyproject_empty(self, tmp_path: Path):
        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text("[project]\nname = 'test'\n")

        analyzer = ExternalDependencyAnalyzer(tmp_path)
        result = analyzer.analyze()

        assert "pyproject.toml" in result.sources
        assert result.total_direct == 0

    def test_parse_pyproject_dependencies(self, tmp_path: Path):
        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text("""
[project]
name = "test"
dependencies = [
    "requests>=2.0",
    "click~=8.0",
]
""")

        analyzer = ExternalDependencyAnalyzer(tmp_path)
        result = analyzer.analyze()

        assert result.total_direct == 2
        names = [d.name for d in result.dependencies]
        assert "requests" in names
        assert "click" in names

    def test_parse_pyproject_optional_deps(self, tmp_path: Path):
        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text("""
[project]
name = "test"
dependencies = []

[project.optional-dependencies]
dev = ["pytest>=8.0", "ruff>=0.1"]
docs = ["mkdocs>=1.0"]
""")

        analyzer = ExternalDependencyAnalyzer(tmp_path)
        result = analyzer.analyze()

        # dev deps go to dev_dependencies
        assert result.total_dev == 2
        # docs go to optional_dependencies
        assert "docs" in result.optional_dependencies
        assert len(result.optional_dependencies["docs"]) == 1

    def test_parse_requirements_txt(self, tmp_path: Path):
        requirements = tmp_path / "requirements.txt"
        requirements.write_text("""
requests>=2.0
click~=8.0
# comment
flask
""")

        analyzer = ExternalDependencyAnalyzer(tmp_path)
        result = analyzer.analyze()

        assert "requirements.txt" in result.sources
        assert result.total_direct == 3

    def test_parse_requirements_dev(self, tmp_path: Path):
        requirements_dev = tmp_path / "requirements-dev.txt"
        requirements_dev.write_text("pytest>=8.0\nruff>=0.1")

        analyzer = ExternalDependencyAnalyzer(tmp_path)
        result = analyzer.analyze()

        assert "requirements-dev.txt" in result.sources
        assert result.total_dev == 2

    def test_parse_dependency_string_simple(self, tmp_path: Path):
        analyzer = ExternalDependencyAnalyzer(tmp_path)
        dep = analyzer._parse_dependency_string("requests")
        assert dep is not None
        assert dep.name == "requests"
        assert dep.version_spec == ""

    def test_parse_dependency_string_versioned(self, tmp_path: Path):
        analyzer = ExternalDependencyAnalyzer(tmp_path)
        dep = analyzer._parse_dependency_string("requests>=2.0,<3.0")
        assert dep is not None
        assert dep.name == "requests"
        assert dep.version_spec == ">=2.0,<3.0"

    def test_parse_dependency_string_with_extras(self, tmp_path: Path):
        analyzer = ExternalDependencyAnalyzer(tmp_path)
        dep = analyzer._parse_dependency_string("uvicorn[standard]>=0.20")
        assert dep is not None
        assert dep.name == "uvicorn"
        assert dep.extras == ["standard"]
        assert dep.version_spec == ">=0.20"

    def test_parse_dependency_string_with_marker(self, tmp_path: Path):
        analyzer = ExternalDependencyAnalyzer(tmp_path)
        dep = analyzer._parse_dependency_string("pywin32>=300; sys_platform=='win32'")
        assert dep is not None
        assert dep.name == "pywin32"
        assert dep.version_spec == ">=300"  # marker stripped

    def test_no_dependency_files(self, tmp_path: Path):
        analyzer = ExternalDependencyAnalyzer(tmp_path)
        result = analyzer.analyze()
        assert result.sources == []
        assert result.total_direct == 0


# =============================================================================
# Factory Function Tests
# =============================================================================


class TestCreateExternalDependencyAnalyzer:
    def test_create_with_default_root(self, monkeypatch, tmp_path: Path):
        monkeypatch.chdir(tmp_path)
        analyzer = create_external_dependency_analyzer()
        assert analyzer.root == tmp_path

    def test_create_with_explicit_root(self, tmp_path: Path):
        analyzer = create_external_dependency_analyzer(root=tmp_path)
        assert analyzer.root == tmp_path
