async-graphql-extension-apollo-tracing
====

<div align="center">
  <!-- CI -->
  <img src="https://github.com/Miaxos/async_graphql_apollo_studio_extension/actions/workflows/ci.yml/badge.svg" />
  <!-- Crates version -->
  <a href="https://crates.io/crates/async-graphql-extension-apollo-tracing">
    <img src="https://img.shields.io/crates/v/async-graphql-extension-apollo-tracing.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Documentation -->
  <a href="https://docs.rs/async-graphql-extension-apollo-tracing/badge.svg">
    <img src="https://docs.rs/async-graphql-extension-apollo-tracing/badge.svg?style=flat-square"
      alt="Documentation" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/async-graphql-extension-apollo-tracing">
    <img src="https://img.shields.io/crates/d/async-graphql-extension-apollo-tracing.svg?style=flat-square"
      alt="Download" />
  </a>
</div>
<br />
<br />


async-graphql-extension-apollo-tracing is an open-source extension for the crates [async_graphql](https://github.com/async-graphql/async-graphql). The purpose of this extension is to provide a simple way to create & send your graphql metrics to [Apollo Studio](https://studio.apollographql.com/).

- [Documentation](https://docs.rs/async-graphql-extension-apollo-tracing/)

_Tested at Rust version: `rustc 1.53.0 (53cb7b09b 2021-06-17)`_

![Apollo Studio with async_graphql](apollo-studio.png?raw=true "Apollo Studio with async_graphql")

## Features

* Tokio 1.0
* Fully support traces & errors
* Batched Protobuf transfer
* Client segmentation
* Additional data to segment your queries by visitors
* Tracing
* Schema export to studio
* Error traces
* Gzip compression

## Crate features

This crate offers the following features, all of which are not activated by default:

- `compression`: Enable the GZIP Compression when sending traces.

## Examples

### Warp

A litle example to how to use it.
If there is something unclear, please write an issue on the repo.
Some examples are going to be written soon.

```rust
use async_graphql_extension_apollo_tracing::{ApolloTracing, ApolloTracingDataExt, HTTPMethod, register::register};

async fn main() -> anyhow::Result<()> {
  ...

  let schema = Schema::build(Query::default(), Mutation::default(), EmptySubscription)
    .data(some_data_needed_for_you)
    .extension(ApolloTracing::new(
      "authorization_token".into(),
      "https://yourdomain.ltd".into(),
      "your_graph@variant".into(),
      "v1.0.0".into(),
      10,
    ))
    .finish();

  register("authorization_token", &schema, "my-allocation-id", "variant", "1.0.0", "staging").await?;
  
  ...

  let client_name = warp::header::optional("apollographql-client-name");
  let client_version = warp::header::optional("apollographql-client-version");
  let env = my_env_filter();

  let graphql_post = warp::post()
      .and(warp::path("graphql"))
      .and(async_graphql_warp::graphql(schema))
      .and(env)
      .and(client_name)
      .and(client_version)
      .and_then(
          |(schema, request): (
              Schema<Query, Mutation, EmptySubscription>,
              async_graphql::Request,
          ),
           env: Environment,
           client_name: Option<String>,
           client_version: Option<String>| async move {
              let userid: Option<String> = env.userid().map(|x| x.to_string());

              Ok::<_, std::convert::Infallible>(async_graphql_warp::Response::from(
                  schema
                      .execute(
                          request.data(ApolloTracingDataExt {
                              userid,
                              path: Some("/graphql".to_string()),
                              host: Some("https://yourdomain.ltd".to_string()),
                              method: Some(HTTPMethod::POST),
                              secure: Some(true),
                              protocol: Some("HTTP/1.1".to_string()),
                              status_code: Some(200),
                              client_name,
                              client_version,
                          })
                              .data(env),
                      )
                      .await,
              ))
          },
      );



}
```

## References

* [GraphQL](https://graphql.org)
* [Async Graphql Crates](https://github.com/async-graphql/async-graphql)
* [Apollo Tracing](https://github.com/apollographql/apollo-tracing)
* [Apollo Server](https://github.com/apollographql/apollo-server)
