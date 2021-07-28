//! # Apollo Studio Extension for Performance Tracing for async_graphql crates
mod compression;
mod packages;
mod proto;
pub mod register;

#[macro_use]
extern crate tracing;
use packages::uname::Uname;
use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::QueryPathSegment;
use chrono::{DateTime, Utc};
use futures::lock::Mutex;
use protobuf::well_known_types::Timestamp;
use protobuf::RepeatedField;
use std::convert::TryFrom;

use async_graphql::extensions::{
    Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery, NextResolve,
    ResolveInfo,
};
use async_graphql::parser::types::{ExecutableDocument, OperationType, Selection};
use async_graphql::{Response, ServerResult, Value, Variables};
use proto::{
    Report, ReportHeader, Trace, Trace_Details, Trace_Error, Trace_HTTP, Trace_HTTP_Method,
    Trace_Location, Trace_Node, Trace_Node_oneof_id, TracesAndStats,
};
use std::convert::TryInto;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::RwLock;

/// Apollo tracing extension for performance tracing
/// https://www.apollographql.com/docs/studio/setup-analytics/#adding-support-to-a-third-party-server-advanced
///
/// Apollo Tracing works by creating Trace from GraphQL calls, which contains extra data about the
/// request being processed. These traces are then batched sent to the Apollo Studio server.
///
/// The extension will start a separate function on a separate thread which will aggregate traces
/// and batch send them.
///
/// To add additional data to your metrics, you should add a ApolloTracingDataExt to your
/// query_data when you process a query with async_graphql.
pub struct ApolloTracing {
    sender: Arc<Sender<(String, Trace)>>,
}

const REPORTING_URL: &str = "https://usage-reporting.api.apollographql.com/api/ingress/traces";
const TARGET_LOG: &str = "apollo-studio-extension";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const RUNTIME_VERSION: &str = "Rust - No runtime version provided yet";

#[derive(Debug, Clone)]
pub enum HTTPMethod {
    UNKNOWN = 0,
    OPTIONS = 1,
    GET = 2,
    HEAD = 3,
    POST = 4,
    PUT = 5,
    DELETE = 6,
    TRACE = 7,
    CONNECT = 8,
    PATCH = 9,
}

impl From<HTTPMethod> for Trace_HTTP_Method {
    fn from(value: HTTPMethod) -> Self {
        match value {
            HTTPMethod::UNKNOWN => Trace_HTTP_Method::UNKNOWN,
            HTTPMethod::OPTIONS => Trace_HTTP_Method::OPTIONS,
            HTTPMethod::GET => Trace_HTTP_Method::GET,
            HTTPMethod::HEAD => Trace_HTTP_Method::HEAD,
            HTTPMethod::POST => Trace_HTTP_Method::POST,
            HTTPMethod::PUT => Trace_HTTP_Method::PUT,
            HTTPMethod::DELETE => Trace_HTTP_Method::DELETE,
            HTTPMethod::TRACE => Trace_HTTP_Method::TRACE,
            HTTPMethod::CONNECT => Trace_HTTP_Method::CONNECT,
            HTTPMethod::PATCH => Trace_HTTP_Method::PATCH,
        }
    }
}

/// This structure must be registered to the Query Data to add context to the apollo metrics.
#[derive(Debug, Clone)]
pub struct ApolloTracingDataExt {
    pub userid: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub path: Option<String>,
    pub host: Option<String>,
    pub method: Option<HTTPMethod>,
    pub secure: Option<bool>,
    pub protocol: Option<String>,
    pub status_code: Option<u32>,
}

impl Default for ApolloTracingDataExt {
    fn default() -> Self {
        ApolloTracingDataExt {
            userid: None,
            client_name: None,
            client_version: None,
            path: None,
            host: None,
            method: None,
            secure: None,
            protocol: None,
            status_code: None,
        }
    }
}

impl ApolloTracing {
    /// We initialize the ApolloTracing Extension by starting our aggregator async function which
    /// will receive every traces and send them to the Apollo Studio Ingress for processing
    ///
    /// autorization_token - Token to send metrics to apollo studio.
    /// hostname - Hostname like yourdomain-graphql-1.io
    /// graph_ref - <ref>@<variant> Graph reference with variant
    /// release_name - Your release version or release name from Git for example
    /// batch_target - The number of traces to batch, it depends on your traffic
    pub fn new(
        authorization_token: String,
        hostname: String,
        graph_ref: String,
        release_name: String,
        batch_target: usize,
    ) -> ApolloTracing {
        let header = Arc::new(ReportHeader {
            uname: Uname::new()
                .ok()
                .map(|x| x.to_string())
                .unwrap_or_else(|| "No uname provided".to_string()),
            hostname,
            graph_ref,
            service_version: release_name,
            agent_version: format!("async-studio-extension {}", VERSION),
            runtime_version: RUNTIME_VERSION.to_string(),
            ..Default::default()
        });

        let client = reqwest::Client::new();
        let (sender, mut receiver) = channel::<(String, Trace)>(batch_target * 3);

        let header_tokio = Arc::clone(&header);

        tokio::spawn(async move {
            let mut hashmap: HashMap<String, TracesAndStats> =
                HashMap::with_capacity(batch_target + 1);
            let mut count = 0;
            while let Some((name, trace)) = receiver.recv().await {
                trace!(target: TARGET_LOG, message = "Trace registered", trace = ?trace, name = ?name);

                // We bufferize traces and create a Full Report every X
                // traces
                match hashmap.get_mut(&name) {
                    Some(previous) => {
                        previous.mut_trace().push(trace);
                    }
                    None => {
                        let mut trace_and_stats = TracesAndStats::new();
                        trace_and_stats.mut_trace().push(trace);

                        hashmap.insert(name, trace_and_stats);
                    }
                }

                count += 1;

                if count > batch_target {
                    use tracing::{field, field::debug, span, Level};

                    let span_batch = span!(
                        Level::DEBUG,
                        "Sending traces by batch to Apollo Studio",
                        response = field::Empty,
                        batched = ?count,
                    );

                    span_batch.in_scope(|| {
                        trace!(target: TARGET_LOG, message = "Sending traces by batch");
                    });

                    let hashmap_to_send = hashmap;
                    hashmap = HashMap::with_capacity(batch_target + 1);

                    let mut report = Report::new();
                    report.set_traces_per_query(hashmap_to_send);
                    report.set_header((*header_tokio).clone());

                    let msg = match protobuf::Message::write_to_bytes(&report) {
                        Ok(message) => message,
                        Err(err) => {
                            span_batch.in_scope(|| {
                                error!(target: TARGET_LOG, error = ?err, report = ?report);
                            });
                            continue;
                        }
                    };

                    let msg = match compression::compress(msg) {
                        Ok(result) => result,
                        Err(e) => {
                            error!(target: TARGET_LOG, message = "An issue happened while GZIP compression", err = ?e);
                            continue;
                        }
                    };

                    let result = client
                        .post(REPORTING_URL)
                        .body(msg)
                        .header("content-type", "application/protobuf")
                        .header("X-Api-Key", &authorization_token)
                        .send()
                        .await;

                    match result {
                        Ok(data) => {
                            span_batch.record("response", &debug(&data));
                            let text = data.text().await;
                            debug!(target: TARGET_LOG, data = ?text);
                        }
                        Err(err) => {
                            let status_code = err.status();
                            error!(target: TARGET_LOG, status = ?status_code, error = ?err);
                        }
                    }
                }
            }
        });

        ApolloTracing {
            sender: Arc::new(sender),
        }
    }
}

impl ExtensionFactory for ApolloTracing {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ApolloTracingExtension {
            inner: Mutex::new(Inner {
                start_time: Utc::now(),
                end_time: Utc::now(),
            }),
            sender: Arc::clone(&self.sender),
            nodes: RwLock::new(HashMap::new()),
            root_node: Arc::new(RwLock::new(Trace_Node::new())),
            operation_name: RwLock::new("schema".to_string()),
        })
    }
}

struct Inner {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

struct ApolloTracingExtension {
    inner: Mutex<Inner>,
    sender: Arc<Sender<(String, Trace)>>,
    nodes: RwLock<HashMap<String, Arc<RwLock<Trace_Node>>>>,
    root_node: Arc<RwLock<Trace_Node>>,
    operation_name: RwLock<String>,
}

#[async_trait::async_trait]
impl Extension for ApolloTracingExtension {
    #[instrument(level = "debug", skip(self, ctx, next))]
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let document = next.run(ctx, query, variables).await?;
        let is_schema = document
            .operations
            .iter()
            .filter(|(_, operation)| operation.node.ty == OperationType::Query)
            .any(|(_, operation)| operation.node.selection_set.node.items.iter().any(|selection| matches!(&selection.node, Selection::Field(field) if field.node.name.node == "__schema")));
        if !is_schema {
            let result: String =
                ctx.stringify_execute_doc(&document, &Variables::from_json(serde_json::json!({})));
            let name = document
                .operations
                .iter()
                .next()
                .map(|x| x.0)
                .flatten()
                .map(|x| x.as_str())
                .unwrap_or("no_name");
            let query_type = format!("# {name}\n {query}", name = name, query = result);
            *self.operation_name.write().await = query_type;
        }
        Ok(document)
    }

    #[instrument(level = "debug", skip(self, ctx, next))]
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let start_time = Utc::now();
        self.inner.lock().await.start_time = start_time;

        let resp = next.run(ctx, operation_name).await;
        // Here every responses are executed
        // The next execute should aggregates a node a not a trace
        let mut inner = self.inner.lock().await;
        inner.end_time = Utc::now();

        let tracing_extension = ctx
            .data::<ApolloTracingDataExt>()
            .ok()
            .cloned()
            .unwrap_or_else(ApolloTracingDataExt::default);
        let client_name = tracing_extension
            .client_name
            .unwrap_or_else(|| "no client name".to_string());
        let client_version = tracing_extension
            .client_version
            .unwrap_or_else(|| "no client version".to_string());
        let userid = tracing_extension
            .userid
            .unwrap_or_else(|| "anonymous".to_string());

        let path = tracing_extension
            .path
            .unwrap_or_else(|| "no path".to_string());
        let host = tracing_extension
            .host
            .unwrap_or_else(|| "no host".to_string());
        let method = tracing_extension.method.unwrap_or(HTTPMethod::UNKNOWN);
        let secure = tracing_extension.secure.unwrap_or(false);
        let protocol = tracing_extension
            .protocol
            .unwrap_or_else(|| "no operation".to_string());
        let status_code = tracing_extension.status_code.unwrap_or(0);

        let mut trace = Trace {
            client_name,
            client_version,
            duration_ns: (inner.end_time - inner.start_time)
                .num_nanoseconds()
                .map(|x| x.try_into().unwrap())
                .unwrap_or(0),
            client_reference_id: userid,
            ..Default::default()
        };

        trace.set_details(Trace_Details {
            operation_name: operation_name
                .map(|x| x.to_string())
                .unwrap_or_else(|| "no operation".to_string()),
            ..Default::default()
        });

        // Should come from Context / Headers
        trace.set_http(Trace_HTTP {
            path,
            host,
            method: Trace_HTTP_Method::from(method),
            secure,
            protocol,
            status_code,
            ..Default::default()
        });

        trace.set_end_time(Timestamp {
            nanos: inner.end_time.timestamp_subsec_nanos().try_into().unwrap(),
            seconds: inner.end_time.timestamp(),
            ..Default::default()
        });

        trace.set_start_time(Timestamp {
            nanos: inner
                .start_time
                .timestamp_subsec_nanos()
                .try_into()
                .unwrap(),
            seconds: inner.start_time.timestamp(),
            ..Default::default()
        });

        let root_node = self.root_node.read().await;
        trace.set_root(root_node.clone());

        let sender = self.sender.clone();

        let operation_name = self.operation_name.read().await.clone();
        tokio::spawn(async move {
            if let Err(e) = sender.send((operation_name, trace)).await {
                error!(error = ?e);
            }
        });
        resp
    }

    #[instrument(level = "debug", skip(self, ctx, info, next))]
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        // We do create a node when it's invoked which we insert at the right place inside the
        // struct.

        let path = info.path_node.to_string_vec().join(".");
        let field_name = info.path_node.field_name().to_string();
        let parent_type = info.parent_type.to_string();
        let return_type = info.return_type.to_string();
        let start_time = Utc::now() - self.inner.lock().await.start_time;
        let path_node = info.path_node;

        let node: Trace_Node = Trace_Node {
            end_time: 0,
            id: match path_node.segment {
                QueryPathSegment::Name(name) => {
                    Some(Trace_Node_oneof_id::response_name(name.to_string()))
                }
                QueryPathSegment::Index(index) => {
                    Some(Trace_Node_oneof_id::index(index.try_into().unwrap_or(0)))
                }
            },
            start_time: match start_time
                .num_nanoseconds()
                .and_then(|x| u64::try_from(x).ok())
            {
                Some(duration) => duration,
                None => Utc::now().timestamp_nanos().try_into().unwrap(),
            },
            parent_type: parent_type.to_string(),
            original_field_name: field_name,
            field_type: return_type,
            ..Default::default()
        };
        let node = Arc::new(RwLock::new(node));
        self.nodes.write().await.insert(path, node.clone());
        let parent_node = path_node.parent.map(|x| x.to_string_vec().join("."));
        // Use the path to create a new node
        // https://github.com/apollographql/apollo-server/blob/291c17e255122d4733b23177500188d68fac55ce/packages/apollo-server-core/src/plugin/traceTreeBuilder.ts
        let res = match next.run(ctx, info).await {
            Ok(res) => Ok(res),
            Err(e) => {
                let mut error = Trace_Error::new();
                error.set_message(e.message.clone());
                error.set_location(RepeatedField::from_vec(
                    e.locations
                        .clone()
                        .into_iter()
                        .map(|x| Trace_Location {
                            line: x.line as u32,
                            column: x.column as u32,
                            ..Default::default()
                        })
                        .collect(),
                ));
                let json = match serde_json::to_string(&e) {
                    Ok(content) => content,
                    Err(e) => serde_json::json!({ "error": format!("{:?}", e) }).to_string(),
                };
                error.set_json(json);
                node.write()
                    .await
                    .set_error(RepeatedField::from_vec(vec![error]));
                Err(e)
            }
        };
        let end_time = Utc::now() - self.inner.lock().await.start_time;

        node.write().await.set_end_time(
            match end_time
                .num_nanoseconds()
                .and_then(|x| u64::try_from(x).ok())
            {
                Some(duration) => duration,
                None => Utc::now().timestamp_nanos().try_into().unwrap(),
            },
        );

        match parent_node {
            None => {
                let mut root_node = self.root_node.write().await;
                let child = &mut *root_node.mut_child();
                let node = node.read().await;
                // Can't copy or pass a ref to Protobuf
                // So we clone
                child.push(node.clone());
            }
            Some(parent) => {
                let nodes = self.nodes.read().await;
                let node_read = &*nodes.get(&parent).unwrap();
                let mut parent = node_read.write().await;
                let child = &mut *parent.mut_child();
                let node = node.read().await;
                // Can't copy or pass a ref to Protobuf
                // So we clone
                child.push(node.clone());
            }
        };

        res
    }
}
