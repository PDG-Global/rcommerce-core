//! Test Reporter Module
//!
//! Provides detailed test reporting with:
//! - Step-by-step test execution tracking
//! - API call logging
//! - Database operation tracking
//! - Entity creation tracking
//! - Assertion results
//! - HTML and JSON report generation

use std::collections::HashMap;
use std::fs;

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// Global test report storage
fn global_reports() -> &'static Mutex<HashMap<String, TestReport>> {
    static REPORTS: OnceLock<Mutex<HashMap<String, TestReport>>> = OnceLock::new();
    REPORTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Detailed test report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub test_name: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_ms: u64,
    pub status: TestStatus,
    pub steps: Vec<TestStep>,
    pub api_calls: Vec<ApiCall>,
    pub db_operations: Vec<DbOperation>,
    pub created_entities: Vec<Entity>,
    pub assertions: Vec<AssertionResult>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Running,
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    pub number: usize,
    pub description: String,
    pub status: StepStatus,
    pub duration_ms: u64,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCall {
    pub method: String,
    pub endpoint: String,
    pub request_body: Option<String>,
    pub response_status: u16,
    pub response_body: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbOperation {
    pub operation: String,
    pub table: String,
    pub details: String,
    pub rows_affected: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_type: String,
    pub id: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    pub description: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

impl TestReport {
    #[allow(dead_code)]
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            start_time: chrono::Utc::now().to_rfc3339(),
            end_time: None,
            duration_ms: 0,
            status: TestStatus::Running,
            steps: Vec::new(),
            api_calls: Vec::new(),
            db_operations: Vec::new(),
            created_entities: Vec::new(),
            assertions: Vec::new(),
            logs: Vec::new(),
        }
    }

    pub fn add_step(&mut self, description: &str) -> usize {
        let step = TestStep {
            number: self.steps.len() + 1,
            description: description.to_string(),
            status: StepStatus::Pending,
            duration_ms: 0,
            details: None,
        };
        self.steps.push(step);
        self.steps.len()
    }

    pub fn complete_step(&mut self, step_num: usize, status: StepStatus, duration_ms: u64) {
        if let Some(step) = self.steps.get_mut(step_num - 1) {
            step.status = status;
            step.duration_ms = duration_ms;
        }
    }

    pub fn add_api_call(&mut self, method: &str, endpoint: &str, status: u16, duration_ms: u64) {
        self.api_calls.push(ApiCall {
            method: method.to_string(),
            endpoint: endpoint.to_string(),
            request_body: None,
            response_status: status,
            response_body: None,
            duration_ms,
        });
    }

    #[allow(dead_code)]
    pub fn add_db_operation(
        &mut self,
        operation: &str,
        table: &str,
        details: &str,
        rows: Option<u64>,
    ) {
        self.db_operations.push(DbOperation {
            operation: operation.to_string(),
            table: table.to_string(),
            details: details.to_string(),
            rows_affected: rows,
        });
    }

    #[allow(dead_code)]
    pub fn add_entity(&mut self, entity_type: &str, id: &str, details: &str) {
        self.created_entities.push(Entity {
            entity_type: entity_type.to_string(),
            id: id.to_string(),
            details: details.to_string(),
        });
    }

    pub fn add_assertion(&mut self, description: &str, passed: bool, expected: &str, actual: &str) {
        self.assertions.push(AssertionResult {
            description: description.to_string(),
            passed,
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }

    pub fn log(&mut self, message: &str) {
        let timestamp = chrono::Utc::now().format("%H:%M:%S%.3f");
        self.logs.push(format!("[{}] {}", timestamp, message));
    }

    pub fn finalize(&mut self, status: TestStatus, duration_ms: u64) {
        self.status = status;
        self.duration_ms = duration_ms;
        self.end_time = Some(chrono::Utc::now().to_rfc3339());
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let mut reports = global_reports()
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock reports: {}", e))?;
        reports.insert(self.test_name.clone(), self.clone());
        Ok(())
    }
}

/// Test reporter that tracks execution
pub struct TestReporter {
    report: TestReport,
    current_step: Option<usize>,
    step_start: Option<std::time::Instant>,
    test_start: std::time::Instant,
}

impl TestReporter {
    pub fn new(test_name: &str) -> Self {
        Self {
            report: TestReport::new(test_name),
            current_step: None,
            step_start: None,
            test_start: std::time::Instant::now(),
        }
    }

    pub fn step(&mut self, description: &str) {
        self.current_step = Some(self.report.add_step(description));
        self.step_start = Some(std::time::Instant::now());
        self.report.log(&format!("Starting: {}", description));
    }

    pub fn step_ok(&mut self) {
        if let (Some(step), Some(start)) = (self.current_step, self.step_start) {
            let duration = start.elapsed().as_millis() as u64;
            self.report
                .complete_step(step, StepStatus::Success, duration);
            self.report
                .log(&format!("Completed step {} in {}ms", step, duration));
        }
    }

    #[allow(dead_code)]
    pub fn step_fail(&mut self, error: &str) {
        if let (Some(step), Some(start)) = (self.current_step, self.step_start) {
            let duration = start.elapsed().as_millis() as u64;
            self.report
                .complete_step(step, StepStatus::Failed, duration);
            self.report.log(&format!("Failed step {}: {}", step, error));
        }
    }

    pub fn api_call(&mut self, method: &str, endpoint: &str, status: u16) {
        self.report.add_api_call(method, endpoint, status, 0);
        self.report
            .log(&format!("API {} {} -> {}", method, endpoint, status));
    }

    #[allow(dead_code)]
    pub fn db_op(&mut self, operation: &str, table: &str, details: &str) {
        self.report
            .add_db_operation(operation, table, details, None);
        self.report
            .log(&format!("DB {} on {}: {}", operation, table, details));
    }

    #[allow(dead_code)]
    pub fn entity(&mut self, entity_type: &str, id: &str, details: &str) {
        self.report.add_entity(entity_type, id, details);
        self.report
            .log(&format!("Created {}: {} ({})", entity_type, id, details));
    }

    pub fn assertion<T: std::fmt::Display>(
        &mut self,
        description: &str,
        passed: bool,
        expected: T,
        actual: T,
    ) {
        let exp_str = expected.to_string();
        let act_str = actual.to_string();
        self.report
            .add_assertion(description, passed, &exp_str, &act_str);
        if passed {
            self.report
                .log(&format!("✓ Assertion passed: {}", description));
        } else {
            self.report.log(&format!(
                "✗ Assertion failed: {} (expected: {}, actual: {})",
                description, exp_str, act_str
            ));
        }
    }

    pub fn finalize(mut self, passed: bool) -> TestReport {
        let status = if passed {
            TestStatus::Passed
        } else {
            TestStatus::Failed
        };
        let duration = self.test_start.elapsed().as_millis() as u64;
        self.report.finalize(status, duration);

        // Save to global state
        self.report.save().ok();

        // Also save immediately to disk (since tests run in separate processes)
        // Use workspace root for reports (tests run from crate directory)
        let output_dir = "../../test-reports/detailed";
        fs::create_dir_all(output_dir).ok();

        // Save JSON
        if let Ok(json) = serde_json::to_string_pretty(&self.report) {
            let path = format!("{}/{}_report.json", output_dir, self.report.test_name);
            fs::write(&path, json).ok();
        }

        // Save HTML
        let html = generate_html_report(&self.report);
        let path = format!("{}/{}_report.html", output_dir, self.report.test_name);
        fs::write(&path, html).ok();

        self.report
    }
}

/// Save all reports to disk
#[allow(dead_code)]
pub fn save_all_reports(output_dir: &str) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)?;

    let reports = global_reports()
        .lock()
        .map_err(|e| anyhow::anyhow!("Failed to lock reports: {}", e))?;

    if reports.is_empty() {
        println!("No test reports to save");
        return Ok(());
    }

    // Save summary JSON
    let summary: Vec<&TestReport> = reports.values().collect();
    let summary_json = serde_json::to_string_pretty(&summary)?;
    fs::write(format!("{}/all_reports.json", output_dir), summary_json)?;

    // Generate individual reports
    for report in reports.values() {
        let json = serde_json::to_string_pretty(report)?;
        fs::write(
            format!("{}/{}_report.json", output_dir, report.test_name),
            json,
        )?;

        let html = generate_html_report(report);
        fs::write(
            format!("{}/{}_report.html", output_dir, report.test_name),
            html,
        )?;
    }

    // Generate index
    let index = generate_index(&reports);
    fs::write(format!("{}/index.html", output_dir), index)?;

    println!("📊 Test reports saved to: {}/", output_dir);
    println!("📊 Summary: {}/index.html", output_dir);

    Ok(())
}

fn generate_html_report(report: &TestReport) -> String {
    let status_class = match report.status {
        TestStatus::Passed => "success",
        TestStatus::Failed => "danger",
        TestStatus::Skipped => "warning",
        TestStatus::Running => "info",
    };

    let status_text = format!("{:?}", report.status);

    let steps_rows: String = report
        .steps
        .iter()
        .map(|s| {
            let status_class = match s.status {
                StepStatus::Success => "success",
                StepStatus::Failed => "danger",
                StepStatus::Pending => "warning",
            };
            format!(
                r#"<tr><td>{}</td><td>{}</td><td>{}ms</td><td><span class="badge bg-{}">{:?}</span></td></tr>"#,
                s.number, s.description, s.duration_ms, status_class, s.status
            )
        })
        .collect();

    let api_rows: String = report
        .api_calls
        .iter()
        .map(|a| {
            let status_class = if a.response_status < 400 { "success" } else { "danger" };
            format!(
                r#"<tr><td>{}</td><td>{}</td><td><span class="badge bg-{}">{}</span></td><td>{}ms</td></tr>"#,
                a.method, a.endpoint, status_class, a.response_status, a.duration_ms
            )
        })
        .collect();

    let entity_rows: String = report
        .created_entities
        .iter()
        .map(|e| {
            format!(
                r#"<tr><td>{}</td><td><code>{}</code></td><td>{}</td></tr>"#,
                e.entity_type, e.id, e.details
            )
        })
        .collect();

    let assertion_rows: String = report
        .assertions
        .iter()
        .map(|a| {
            let status_class = if a.passed { "success" } else { "danger" };
            let status_text = if a.passed { "PASS" } else { "FAIL" };
            format!(
                r#"<tr><td>{}</td><td><span class="badge bg-{}">{}</span></td><td>{}</td><td>{}</td></tr>"#,
                a.description, status_class, status_text, a.expected, a.actual
            )
        })
        .collect();

    let logs_html: String = report
        .logs
        .iter()
        .map(|l| format!("<div class='log-line'>{}</div>", l))
        .collect();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Test Report: {}</title>
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.1.3/dist/css/bootstrap.min.css" rel="stylesheet">
    <style>
        body {{ padding: 20px; background: #f5f5f5; }}
        .container {{ max-width: 1200px; }}
        .card {{ margin-bottom: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .log-line {{ font-family: monospace; font-size: 12px; padding: 2px 0; border-bottom: 1px solid #eee; }}
        .badge {{ font-size: 12px; }}
    </style>
</head>
<body>
    <div class="container">
        <h1 class="mb-4">Test Report: {}</h1>
        
        <div class="alert alert-{} mb-4">
            <strong>Status:</strong> {} | 
            <strong>Duration:</strong> {}ms | 
            <strong>Started:</strong> {} | 
            <strong>Ended:</strong> {}
        </div>

        <div class="row">
            <div class="col-md-6">
                <div class="card">
                    <div class="card-header"><h5>Test Steps</h5></div>
                    <div class="card-body p-0">
                        <table class="table table-striped mb-0">
                            <thead><tr><th>#</th><th>Description</th><th>Time</th><th>Status</th></tr></thead>
                            <tbody>{}</tbody>
                        </table>
                    </div>
                </div>
            </div>
            
            <div class="col-md-6">
                <div class="card">
                    <div class="card-header"><h5>API Calls</h5></div>
                    <div class="card-body p-0">
                        <table class="table table-striped mb-0">
                            <thead><tr><th>Method</th><th>Endpoint</th><th>Status</th><th>Time</th></tr></thead>
                            <tbody>{}</tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>

        <div class="card">
            <div class="card-header"><h5>Created Entities</h5></div>
            <div class="card-body p-0">
                <table class="table table-striped mb-0">
                    <thead><tr><th>Type</th><th>ID</th><th>Details</th></tr></thead>
                    <tbody>{}</tbody>
                </table>
            </div>
        </div>

        <div class="card">
            <div class="card-header"><h5>Assertions</h5></div>
            <div class="card-body p-0">
                <table class="table table-striped mb-0">
                    <thead><tr><th>Description</th><th>Result</th><th>Expected</th><th>Actual</th></tr></thead>
                    <tbody>{}</tbody>
                </table>
            </div>
        </div>

        <div class="card">
            <div class="card-header"><h5>Execution Log</h5></div>
            <div class="card-body" style="max-height: 400px; overflow-y: auto;">
                {}
            </div>
        </div>
    </div>
</body>
</html>"#,
        report.test_name,
        report.test_name,
        status_class,
        status_text,
        report.duration_ms,
        report.start_time,
        report.end_time.as_deref().unwrap_or("N/A"),
        steps_rows,
        api_rows,
        entity_rows,
        assertion_rows,
        logs_html
    )
}

#[allow(dead_code)]
fn generate_index(reports: &HashMap<String, TestReport>) -> String {
    let rows: String = reports
        .values()
        .map(|r| {
            let status_class = match r.status {
                TestStatus::Passed => "success",
                TestStatus::Failed => "danger",
                TestStatus::Skipped => "warning",
                TestStatus::Running => "info",
            };
            let status_text = format!("{:?}", r.status);
            format!(
                r#"<tr>
                    <td>{}</td>
                    <td><span class="badge bg-{}">{}</span></td>
                    <td>{}ms</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td><a href="{}_report.html" class="btn btn-sm btn-primary">View</a></td>
                </tr>"#,
                r.test_name,
                status_class,
                status_text,
                r.duration_ms,
                r.steps.len(),
                r.api_calls.len(),
                r.assertions.len(),
                r.test_name
            )
        })
        .collect();

    let total = reports.len();
    let passed = reports
        .values()
        .filter(|r| matches!(r.status, TestStatus::Passed))
        .count();
    let failed = reports
        .values()
        .filter(|r| matches!(r.status, TestStatus::Failed))
        .count();
    let pass_rate = if total > 0 { (passed * 100) / total } else { 0 };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Integration Test Reports</title>
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.1.3/dist/css/bootstrap.min.css" rel="stylesheet">
    <style>
        body {{ padding: 20px; background: #f5f5f5; }}
        .container {{ max-width: 1400px; }}
        .stats-card {{ text-align: center; padding: 20px; }}
        .stats-card h2 {{ margin: 0; font-size: 36px; }}
    </style>
</head>
<body>
    <div class="container">
        <h1 class="mb-4">Integration Test Reports</h1>
        
        <div class="row mb-4">
            <div class="col-md-3">
                <div class="card stats-card">
                    <h2>{}</h2>
                    <p class="text-muted">Total Tests</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card stats-card text-success">
                    <h2>{}</h2>
                    <p class="text-muted">Passed</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card stats-card text-danger">
                    <h2>{}</h2>
                    <p class="text-muted">Failed</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="card stats-card text-info">
                    <h2>{}%</h2>
                    <p class="text-muted">Pass Rate</p>
                </div>
            </div>
        </div>

        <div class="card">
            <div class="card-header"><h5>Test Results</h5></div>
            <div class="card-body p-0">
                <table class="table table-striped mb-0">
                    <thead>
                        <tr>
                            <th>Test Name</th>
                            <th>Status</th>
                            <th>Duration</th>
                            <th>Steps</th>
                            <th>API Calls</th>
                            <th>Assertions</th>
                            <th>Action</th>
                        </tr>
                    </thead>
                    <tbody>{}</tbody>
                </table>
            </div>
        </div>
    </div>
</body>
</html>"#,
        total, passed, failed, pass_rate, rows
    )
}
