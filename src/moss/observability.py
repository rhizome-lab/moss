"""Observability module for Moss.

This module provides metrics collection and distributed tracing capabilities
for monitoring and debugging Moss applications in production.
"""

import time
import uuid
from collections import defaultdict
from collections.abc import Callable
from contextlib import contextmanager
from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class MetricType(Enum):
    """Types of metrics."""

    COUNTER = "counter"
    GAUGE = "gauge"
    HISTOGRAM = "histogram"
    SUMMARY = "summary"


@dataclass
class MetricValue:
    """A single metric value with metadata."""

    name: str
    value: float
    metric_type: MetricType
    labels: dict[str, str] = field(default_factory=dict)
    timestamp: float = field(default_factory=time.time)


@dataclass
class HistogramBucket:
    """Histogram bucket for distribution tracking."""

    le: float  # Less than or equal to
    count: int = 0


class MetricsCollector:
    """Collects and exposes metrics."""

    def __init__(self):
        """Initialize the metrics collector."""
        self._counters: dict[str, float] = defaultdict(float)
        self._gauges: dict[str, float] = {}
        self._histograms: dict[str, list[float]] = defaultdict(list)
        self._labels: dict[str, dict[str, str]] = {}

    def _make_key(self, name: str, labels: dict[str, str] | None = None) -> str:
        """Create a unique key for a metric with labels."""
        if not labels:
            return name
        label_str = ",".join(f"{k}={v}" for k, v in sorted(labels.items()))
        return f"{name}{{{label_str}}}"

    def counter(
        self,
        name: str,
        value: float = 1,
        labels: dict[str, str] | None = None,
    ) -> None:
        """Increment a counter metric.

        Args:
            name: Metric name
            value: Value to add (default 1)
            labels: Optional labels for the metric
        """
        key = self._make_key(name, labels)
        self._counters[key] += value
        if labels:
            self._labels[key] = labels

    def gauge(
        self,
        name: str,
        value: float,
        labels: dict[str, str] | None = None,
    ) -> None:
        """Set a gauge metric.

        Args:
            name: Metric name
            value: Current value
            labels: Optional labels for the metric
        """
        key = self._make_key(name, labels)
        self._gauges[key] = value
        if labels:
            self._labels[key] = labels

    def histogram(
        self,
        name: str,
        value: float,
        labels: dict[str, str] | None = None,
    ) -> None:
        """Record a histogram observation.

        Args:
            name: Metric name
            value: Observed value
            labels: Optional labels for the metric
        """
        key = self._make_key(name, labels)
        self._histograms[key].append(value)
        if labels:
            self._labels[key] = labels

    @contextmanager
    def timer(self, name: str, labels: dict[str, str] | None = None):
        """Context manager for timing operations.

        Args:
            name: Metric name for the duration
            labels: Optional labels for the metric

        Yields:
            Dict to store additional data
        """
        start = time.perf_counter()
        result: dict[str, Any] = {}
        try:
            yield result
        finally:
            duration = time.perf_counter() - start
            self.histogram(f"{name}_seconds", duration, labels)
            self.counter(f"{name}_total", labels=labels)
            result["duration_seconds"] = duration

    def get_metrics(self) -> list[MetricValue]:
        """Get all collected metrics.

        Returns:
            List of MetricValue objects
        """
        metrics = []

        # Counters
        for key, value in self._counters.items():
            name = key.split("{")[0] if "{" in key else key
            labels = self._labels.get(key, {})
            metrics.append(
                MetricValue(
                    name=name,
                    value=value,
                    metric_type=MetricType.COUNTER,
                    labels=labels,
                )
            )

        # Gauges
        for key, value in self._gauges.items():
            name = key.split("{")[0] if "{" in key else key
            labels = self._labels.get(key, {})
            metrics.append(
                MetricValue(
                    name=name,
                    value=value,
                    metric_type=MetricType.GAUGE,
                    labels=labels,
                )
            )

        # Histograms (expose count and sum)
        for key, values in self._histograms.items():
            name = key.split("{")[0] if "{" in key else key
            labels = self._labels.get(key, {})
            if values:
                metrics.append(
                    MetricValue(
                        name=f"{name}_count",
                        value=len(values),
                        metric_type=MetricType.HISTOGRAM,
                        labels=labels,
                    )
                )
                metrics.append(
                    MetricValue(
                        name=f"{name}_sum",
                        value=sum(values),
                        metric_type=MetricType.HISTOGRAM,
                        labels=labels,
                    )
                )

        return metrics

    def to_prometheus(self) -> str:
        """Export metrics in Prometheus text format.

        Returns:
            Prometheus-formatted metrics string
        """
        lines = []

        # Counters
        for key, value in self._counters.items():
            lines.append(f"{key} {value}")

        # Gauges
        for key, value in self._gauges.items():
            lines.append(f"{key} {value}")

        # Histograms
        for key, values in self._histograms.items():
            if values:
                lines.append(f"{key}_count {len(values)}")
                lines.append(f"{key}_sum {sum(values)}")

        return "\n".join(lines)

    def reset(self) -> None:
        """Reset all metrics."""
        self._counters.clear()
        self._gauges.clear()
        self._histograms.clear()
        self._labels.clear()


@dataclass
class Span:
    """A single span in a trace."""

    trace_id: str
    span_id: str
    name: str
    parent_id: str | None = None
    start_time: float = field(default_factory=time.time)
    end_time: float | None = None
    status: str = "OK"
    attributes: dict[str, Any] = field(default_factory=dict)
    events: list[dict[str, Any]] = field(default_factory=list)

    @property
    def duration_ms(self) -> float | None:
        """Get span duration in milliseconds."""
        if self.end_time is None:
            return None
        return (self.end_time - self.start_time) * 1000

    def set_attribute(self, key: str, value: Any) -> None:
        """Set a span attribute."""
        self.attributes[key] = value

    def add_event(self, name: str, attributes: dict[str, Any] | None = None) -> None:
        """Add an event to the span."""
        self.events.append(
            {
                "name": name,
                "timestamp": time.time(),
                "attributes": attributes or {},
            }
        )

    def end(self, status: str = "OK") -> None:
        """End the span."""
        self.end_time = time.time()
        self.status = status


class Tracer:
    """Distributed tracing implementation."""

    def __init__(self, service_name: str = "moss"):
        """Initialize the tracer.

        Args:
            service_name: Name of the service for traces
        """
        self.service_name = service_name
        self._current_span: Span | None = None
        self._spans: list[Span] = []
        self._exporters: list[Callable[[Span], None]] = []

    def add_exporter(self, exporter: Callable[[Span], None]) -> None:
        """Add a span exporter.

        Args:
            exporter: Function to call with completed spans
        """
        self._exporters.append(exporter)

    def _generate_id(self) -> str:
        """Generate a unique ID for traces/spans."""
        return uuid.uuid4().hex[:16]

    @contextmanager
    def start_span(self, name: str, parent: Span | None = None):
        """Start a new span.

        Args:
            name: Name of the span
            parent: Optional parent span

        Yields:
            The created Span object
        """
        # Use parent's trace_id or generate new one
        if parent:
            trace_id = parent.trace_id
            parent_id = parent.span_id
        elif self._current_span:
            trace_id = self._current_span.trace_id
            parent_id = self._current_span.span_id
        else:
            trace_id = self._generate_id()
            parent_id = None

        span = Span(
            trace_id=trace_id,
            span_id=self._generate_id(),
            name=name,
            parent_id=parent_id,
        )

        # Set as current span
        previous_span = self._current_span
        self._current_span = span

        try:
            yield span
        except Exception as e:
            span.set_attribute("error", True)
            span.set_attribute("error.message", str(e))
            span.end(status="ERROR")
            raise
        else:
            span.end(status="OK")
        finally:
            self._current_span = previous_span
            self._spans.append(span)

            # Export span
            for exporter in self._exporters:
                exporter(span)

    def get_current_span(self) -> Span | None:
        """Get the current active span."""
        return self._current_span

    def get_spans(self) -> list[Span]:
        """Get all recorded spans."""
        return self._spans.copy()

    def clear_spans(self) -> None:
        """Clear recorded spans."""
        self._spans.clear()


# Global instances
_metrics: MetricsCollector | None = None
_tracer: Tracer | None = None


def get_metrics() -> MetricsCollector:
    """Get the global metrics collector."""
    global _metrics
    if _metrics is None:
        _metrics = MetricsCollector()
    return _metrics


def get_tracer(service_name: str = "moss") -> Tracer:
    """Get the global tracer.

    Args:
        service_name: Service name for the tracer

    Returns:
        Tracer instance
    """
    global _tracer
    if _tracer is None:
        _tracer = Tracer(service_name)
    return _tracer


def reset_observability() -> None:
    """Reset all observability state."""
    global _metrics, _tracer
    if _metrics:
        _metrics.reset()
    if _tracer:
        _tracer.clear_spans()


# Convenience decorators
def traced(name: str | None = None):
    """Decorator to trace a function.

    Args:
        name: Optional span name (defaults to function name)
    """

    def decorator(func: Callable) -> Callable:
        span_name = name or func.__name__

        def wrapper(*args: Any, **kwargs: Any) -> Any:
            tracer = get_tracer()
            with tracer.start_span(span_name) as span:
                span.set_attribute("function", func.__name__)
                return func(*args, **kwargs)

        return wrapper

    return decorator


def timed(name: str | None = None, labels: dict[str, str] | None = None):
    """Decorator to time a function.

    Args:
        name: Optional metric name (defaults to function name)
        labels: Optional labels for the metric
    """

    def decorator(func: Callable) -> Callable:
        metric_name = name or func.__name__

        def wrapper(*args: Any, **kwargs: Any) -> Any:
            metrics = get_metrics()
            with metrics.timer(metric_name, labels):
                return func(*args, **kwargs)

        return wrapper

    return decorator
