"""Tests for observability module."""

import time

from moss.observability import (
    MetricsCollector,
    MetricType,
    Span,
    Tracer,
    get_metrics,
    get_tracer,
    reset_observability,
    timed,
    traced,
)


class TestMetricsCollector:
    """Tests for MetricsCollector."""

    def test_counter(self):
        metrics = MetricsCollector()
        metrics.counter("requests_total")
        metrics.counter("requests_total")
        metrics.counter("requests_total", 5)

        all_metrics = metrics.get_metrics()
        counter = next(m for m in all_metrics if m.name == "requests_total")
        assert counter.value == 7
        assert counter.metric_type == MetricType.COUNTER

    def test_counter_with_labels(self):
        metrics = MetricsCollector()
        metrics.counter("http_requests", labels={"method": "GET"})
        metrics.counter("http_requests", labels={"method": "POST"})
        metrics.counter("http_requests", labels={"method": "GET"})

        all_metrics = metrics.get_metrics()
        get_counter = next(
            m for m in all_metrics if m.name == "http_requests" and m.labels.get("method") == "GET"
        )
        post_counter = next(
            m for m in all_metrics if m.name == "http_requests" and m.labels.get("method") == "POST"
        )
        assert get_counter.value == 2
        assert post_counter.value == 1

    def test_gauge(self):
        metrics = MetricsCollector()
        metrics.gauge("temperature", 25.5)
        metrics.gauge("temperature", 26.0)

        all_metrics = metrics.get_metrics()
        gauge = next(m for m in all_metrics if m.name == "temperature")
        assert gauge.value == 26.0
        assert gauge.metric_type == MetricType.GAUGE

    def test_histogram(self):
        metrics = MetricsCollector()
        metrics.histogram("request_duration", 0.1)
        metrics.histogram("request_duration", 0.2)
        metrics.histogram("request_duration", 0.3)

        all_metrics = metrics.get_metrics()
        count = next(m for m in all_metrics if m.name == "request_duration_count")
        total = next(m for m in all_metrics if m.name == "request_duration_sum")
        assert count.value == 3
        assert abs(total.value - 0.6) < 0.001

    def test_timer(self):
        metrics = MetricsCollector()

        with metrics.timer("operation") as result:
            time.sleep(0.01)

        assert "duration_seconds" in result
        assert result["duration_seconds"] >= 0.01

        all_metrics = metrics.get_metrics()
        assert any(m.name == "operation_seconds_count" for m in all_metrics)
        assert any(m.name == "operation_total" for m in all_metrics)

    def test_to_prometheus(self):
        metrics = MetricsCollector()
        metrics.counter("requests_total", 10)
        metrics.gauge("active_connections", 5)

        prometheus = metrics.to_prometheus()
        assert "requests_total 10" in prometheus
        assert "active_connections 5" in prometheus

    def test_reset(self):
        metrics = MetricsCollector()
        metrics.counter("test_counter")
        metrics.gauge("test_gauge", 1)

        metrics.reset()

        all_metrics = metrics.get_metrics()
        assert len(all_metrics) == 0


class TestSpan:
    """Tests for Span."""

    def test_create_span(self):
        span = Span(
            trace_id="trace123",
            span_id="span456",
            name="test_operation",
        )
        assert span.trace_id == "trace123"
        assert span.span_id == "span456"
        assert span.name == "test_operation"
        assert span.status == "OK"

    def test_span_duration(self):
        span = Span(trace_id="t1", span_id="s1", name="test")
        span.start_time = 1000.0
        span.end_time = 1000.5
        assert span.duration_ms == 500.0

    def test_span_duration_not_ended(self):
        span = Span(trace_id="t1", span_id="s1", name="test")
        assert span.duration_ms is None

    def test_set_attribute(self):
        span = Span(trace_id="t1", span_id="s1", name="test")
        span.set_attribute("user_id", "123")
        span.set_attribute("operation", "query")
        assert span.attributes["user_id"] == "123"
        assert span.attributes["operation"] == "query"

    def test_add_event(self):
        span = Span(trace_id="t1", span_id="s1", name="test")
        span.add_event("checkpoint", {"data": "value"})
        assert len(span.events) == 1
        assert span.events[0]["name"] == "checkpoint"
        assert span.events[0]["attributes"]["data"] == "value"

    def test_end_span(self):
        span = Span(trace_id="t1", span_id="s1", name="test")
        span.end(status="OK")
        assert span.end_time is not None
        assert span.status == "OK"


class TestTracer:
    """Tests for Tracer."""

    def test_create_tracer(self):
        tracer = Tracer("test_service")
        assert tracer.service_name == "test_service"

    def test_start_span(self):
        tracer = Tracer()

        with tracer.start_span("test_operation") as span:
            assert span.name == "test_operation"
            assert span.trace_id is not None
            assert span.span_id is not None

        assert span.end_time is not None
        assert span.status == "OK"

    def test_nested_spans(self):
        tracer = Tracer()

        with tracer.start_span("parent") as parent:
            with tracer.start_span("child") as child:
                assert child.parent_id == parent.span_id
                assert child.trace_id == parent.trace_id

    def test_span_error(self):
        tracer = Tracer()

        try:
            with tracer.start_span("failing") as span:
                raise ValueError("test error")
        except ValueError:
            pass

        assert span.status == "ERROR"
        assert span.attributes.get("error") is True
        assert "test error" in span.attributes.get("error.message", "")

    def test_exporter(self):
        tracer = Tracer()
        exported_spans = []

        tracer.add_exporter(lambda s: exported_spans.append(s))

        with tracer.start_span("test"):
            pass

        assert len(exported_spans) == 1
        assert exported_spans[0].name == "test"

    def test_get_current_span(self):
        tracer = Tracer()

        assert tracer.get_current_span() is None

        with tracer.start_span("outer") as outer:
            assert tracer.get_current_span() is outer
            with tracer.start_span("inner") as inner:
                assert tracer.get_current_span() is inner
            assert tracer.get_current_span() is outer

        assert tracer.get_current_span() is None

    def test_get_spans(self):
        tracer = Tracer()

        with tracer.start_span("span1"):
            pass
        with tracer.start_span("span2"):
            pass

        spans = tracer.get_spans()
        assert len(spans) == 2

    def test_clear_spans(self):
        tracer = Tracer()

        with tracer.start_span("test"):
            pass

        tracer.clear_spans()
        assert len(tracer.get_spans()) == 0


class TestGlobalFunctions:
    """Tests for global observability functions."""

    def test_get_metrics_singleton(self):
        m1 = get_metrics()
        m2 = get_metrics()
        assert m1 is m2

    def test_get_tracer_singleton(self):
        t1 = get_tracer()
        t2 = get_tracer()
        assert t1 is t2

    def test_reset_observability(self):
        metrics = get_metrics()
        tracer = get_tracer()

        metrics.counter("test")
        with tracer.start_span("test"):
            pass

        reset_observability()

        # Metrics and spans should be cleared
        assert len(metrics.get_metrics()) == 0
        assert len(tracer.get_spans()) == 0


class TestDecorators:
    """Tests for decorators."""

    def test_traced_decorator(self):
        tracer = get_tracer()
        tracer.clear_spans()

        @traced("my_function")
        def sample_function():
            return 42

        result = sample_function()
        assert result == 42

        spans = tracer.get_spans()
        assert len(spans) >= 1
        assert any(s.name == "my_function" for s in spans)

    def test_traced_decorator_default_name(self):
        tracer = get_tracer()
        tracer.clear_spans()

        @traced()
        def another_function():
            return "test"

        another_function()

        spans = tracer.get_spans()
        assert any(s.name == "another_function" for s in spans)

    def test_timed_decorator(self):
        reset_observability()
        metrics = get_metrics()

        @timed("my_operation")
        def slow_function():
            time.sleep(0.01)
            return "done"

        result = slow_function()
        assert result == "done"

        all_metrics = metrics.get_metrics()
        assert any(m.name == "my_operation_seconds_count" for m in all_metrics)
