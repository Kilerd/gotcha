//! Query and path parameters are named as serde reads them, not as the schema writes them: a
//! request built from the documented parameters is accepted.
#![cfg(feature = "openapi")]
// The fields are only ever read by serde.
#![allow(dead_code)]

use gotcha::axum::body::Body;
use gotcha::axum::extract::{Path, Query};
use gotcha::axum::http::{Request, StatusCode, Uri};
use gotcha::axum::routing::get;
use gotcha::axum::Router;
use gotcha::{Either, ParameterProvider, Schematic};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

fn parameter_names<P: ParameterProvider>(url: &str) -> Vec<String> {
    let Either::Left(parameters) = P::generate(url.to_owned()) else {
        panic!("{url} takes parameters, not a body");
    };
    parameters.into_iter().map(|parameter| parameter.name).collect()
}

/// Assert `T` documents `expected` and serde reads a query built from what it documents.
#[track_caller]
fn assert_query_parameters<T>(expected: &[&str])
where
    T: Schematic + DeserializeOwned,
{
    let names = parameter_names::<Query<T>>("/");
    assert_eq!(names, expected);
    let query = names.iter().map(|name| format!("{name}=1")).collect::<Vec<_>>().join("&");
    let uri: Uri = format!("/?{query}").parse().expect("a valid query");
    if let Err(error) = Query::<T>::try_from_uri(&uri) {
        panic!("serde rejects the documented query {query}: {error}");
    }
}

#[derive(Schematic, Deserialize)]
struct WriteRenamed {
    #[serde(rename(serialize = "written"))]
    value: u32,
    page_size: u32,
}

#[derive(Schematic, Deserialize)]
struct ReadRenamed {
    #[serde(rename(deserialize = "read"))]
    value: u32,
}

#[derive(Schematic, Deserialize)]
struct SplitRenamed {
    #[serde(rename(serialize = "written", deserialize = "read"))]
    value: u32,
}

#[derive(Schematic, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
struct WriteConvention {
    page_size: u32,
}

#[derive(Schematic, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct ReadConvention {
    page_size: u32,
}

#[derive(Schematic, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
struct SplitConvention {
    page_size: u32,
}

#[derive(Schematic, Deserialize)]
struct FlattensReadRenamed {
    other: u32,
    #[serde(flatten)]
    inner: ReadRenamed,
}

#[derive(Schematic, Deserialize)]
struct NewtypeOfReadRenamed(ReadRenamed);

/// serde reads the proxy here, so the rename on the converted type has no effect on the query.
#[derive(Schematic, Serialize, Deserialize, Clone)]
struct PageProxy {
    page: u32,
}

#[derive(Schematic, Serialize, Deserialize, Clone)]
#[serde(from = "PageProxy", into = "PageProxy")]
struct ConvertedBothWays {
    #[serde(rename(deserialize = "other"))]
    page: u32,
}

impl From<PageProxy> for ConvertedBothWays {
    fn from(proxy: PageProxy) -> Self {
        ConvertedBothWays { page: proxy.page }
    }
}

impl From<ConvertedBothWays> for PageProxy {
    fn from(converted: ConvertedBothWays) -> Self {
        PageProxy { page: converted.page }
    }
}

#[derive(Schematic, Deserialize)]
#[serde(from = "PageProxy")]
struct ConvertedFrom {
    #[serde(rename(deserialize = "other"))]
    page: u32,
}

impl From<PageProxy> for ConvertedFrom {
    fn from(proxy: PageProxy) -> Self {
        ConvertedFrom { page: proxy.page }
    }
}

#[derive(Schematic, Deserialize)]
#[serde(try_from = "PageProxy")]
struct ConvertedTryFrom {
    #[serde(rename(deserialize = "other"))]
    page: u32,
}

impl TryFrom<PageProxy> for ConvertedTryFrom {
    type Error = String;

    fn try_from(proxy: PageProxy) -> Result<Self, String> {
        Ok(ConvertedTryFrom { page: proxy.page })
    }
}

/// `into` only changes what serde writes, so the rename is what it reads.
#[derive(Schematic, Serialize, Deserialize, Clone)]
#[serde(into = "PageProxy")]
struct ConvertedInto {
    #[serde(rename(deserialize = "other"))]
    page: u32,
}

impl From<ConvertedInto> for PageProxy {
    fn from(converted: ConvertedInto) -> Self {
        PageProxy { page: converted.page }
    }
}

#[test]
fn a_rename_serde_does_not_read_through_a_conversion_is_no_parameter() {
    assert_query_parameters::<ConvertedBothWays>(&["page"]);
    assert_query_parameters::<ConvertedFrom>(&["page"]);
    assert_query_parameters::<ConvertedTryFrom>(&["page"]);
    assert_query_parameters::<ConvertedInto>(&["other"]);
}

#[test]
fn query_parameters_use_the_names_serde_reads() {
    assert_query_parameters::<WriteRenamed>(&["value", "page_size"]);
    assert_query_parameters::<ReadRenamed>(&["read"]);
    assert_query_parameters::<SplitRenamed>(&["read"]);
    assert_query_parameters::<WriteConvention>(&["page_size"]);
    assert_query_parameters::<ReadConvention>(&["pageSize"]);
    assert_query_parameters::<SplitConvention>(&["page-size"]);
}

#[test]
fn query_parameters_follow_flatten_and_wrappers() {
    assert_eq!(parameter_names::<Query<NewtypeOfReadRenamed>>("/"), ["read"]);
    assert_eq!(parameter_names::<Query<FlattensReadRenamed>>("/"), ["other", "read"]);
    assert_eq!(parameter_names::<Query<Option<ReadRenamed>>>("/"), ["read"]);
    assert_eq!(parameter_names::<Query<Box<ReadRenamed>>>("/"), ["read"]);
}

async fn path_status<T>(route: &str, uri: &str) -> StatusCode
where
    T: DeserializeOwned + Send + 'static,
{
    let router = Router::new().route(route, get(|_: Path<T>| async {}));
    let request = Request::builder().uri(uri).body(Body::empty()).expect("a valid request");
    router.oneshot(request).await.expect("the router is infallible").status()
}

#[derive(Schematic, Deserialize)]
struct PathRead {
    #[serde(rename(deserialize = "id"))]
    value: u32,
}

#[derive(Schematic, Deserialize)]
struct PathWrite {
    #[serde(rename(serialize = "written"))]
    id: u32,
}

#[derive(Schematic, Deserialize)]
struct PathSplit {
    #[serde(rename(serialize = "written", deserialize = "id"))]
    value: u32,
}

#[tokio::test]
async fn path_parameters_match_the_segments_serde_reads() {
    assert_eq!(parameter_names::<Path<PathRead>>("/{id}"), ["id"]);
    assert_eq!(path_status::<PathRead>("/{id}", "/1").await, StatusCode::OK);
    assert_eq!(parameter_names::<Path<PathWrite>>("/{id}"), ["id"]);
    assert_eq!(path_status::<PathWrite>("/{id}", "/1").await, StatusCode::OK);
    assert_eq!(parameter_names::<Path<PathSplit>>("/{id}"), ["id"]);
    assert_eq!(path_status::<PathSplit>("/{id}", "/1").await, StatusCode::OK);
}
