use juniper::{GraphQLType, RootNode, http::GraphQLRequest};

use hyper::{
  self,
  Body, Request, Response, Server, Method, StatusCode,
  rt::Future,
  service::service_fn,
};
use futures::{future, Stream};
use serde_json;
use std::{
  sync::Arc,
  fmt,
  error::Error as StdError,
};

#[derive(Debug)]
enum Error {
  Hyper(hyper::error::Error),
  Serde(serde_json::Error),
}

impl StdError for Error {}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      Error::Hyper(err) => err.fmt(f),
      Error::Serde(err) => err.fmt(f),
    }
  }
}

pub struct Apollo<CtxFactory, CtxT, Query, Mutation>
  where
      CtxFactory: Fn(&Request<Body>) -> CtxT + Send + Sync + 'static,
      CtxT: 'static,
      Query: GraphQLType<Context=CtxT, TypeInfo=()> + Send + Sync + 'static,
      Mutation: GraphQLType<Context=CtxT, TypeInfo=()> + Send + Sync + 'static,
{
  root_node: RootNode<'static, Query, Mutation>,
  context_factory: CtxFactory,
}

impl<CtxFactory, CtxT, Query, Mutation> Apollo<CtxFactory, CtxT, Query, Mutation>
  where
      CtxFactory: Fn(&Request<Body>) -> CtxT + Send + Sync + 'static,
      CtxT: 'static,
      Query: GraphQLType<Context=CtxT, TypeInfo=()> + Send + Sync + 'static,
      Mutation: GraphQLType<Context=CtxT, TypeInfo=()> + Send + Sync + 'static,
{
  pub fn new(root_node: RootNode<'static, Query, Mutation>, context_factory: CtxFactory) -> Self {
    Apollo {
      root_node,
      context_factory,
    }
  }

  pub fn start(self, host: Option<&str>) {
    let host = host.unwrap_or("0.0.0.0:8080").parse().unwrap();

    hyper::rt::run(future::lazy(move || {
      let apollo = Apollo::new(self.root_node, self.context_factory);

      let apollo = Arc::new(apollo);

      let new_service = move || {
//        let apollo = Arc::clone(&apollo);
        service_fn(|req| {
          Arc::clone(&apollo).handle(req)
        })
      };

      let server = Server::bind(&host)
          .serve(new_service)
          .map_err(|e| {
            error!("server error: {}", e);
          });

      info!("GraphQL server started on http://{}", host);

      server
    }));
  }

  fn handle(&'static self, req: Request<Body>) -> Box<Future<Item=Response<Body>, Error=hyper::Error> + Send> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
      (&Method::GET, "/") => {
        *response.body_mut() = Body::from(playground("/graphql"));
      }
      (&Method::POST, "/graphql") => {
        let gql_res = req
            .body()
            .concat2()
            .map_err(Error::Hyper)
            .and_then(move |b| {
              let request = serde_json::from_slice::<GraphQLRequest>(b.as_ref()).map_err(Error::Serde)?;
              let context = (self.context_factory)(&req);
              let res = request.execute(&self.root_node, &context);
              let json = serde_json::to_string_pretty(&res).unwrap();
              Ok(Response::new(Body::from(json)))
            })
            .then(convert_error);

        return Box::new(gql_res);
      }
      _ => {
        *response.status_mut() = StatusCode::NOT_FOUND;
      }
    };

    Box::new(future::ok(response))
  }
}

fn convert_error(res: Result<Response<Body>, Error>) -> Result<Response<Body>, hyper::Error> {
  match res {
    Err(err) => {
      match err {
        Error::Serde(err) => {
          let body = format!(r#"{{"status": 500, "description": {}}}"#, err);

          Ok(Response::builder()
              .status(StatusCode::INTERNAL_SERVER_ERROR)
              .header("Content-Type", "application/json")
              .header("Content-Length", body.len())
              .body(Body::from(body))
              .unwrap())
        }
        Error::Hyper(err) => Err(err),
      }
    }
    Ok(v) => Ok(v),
  }
}

//impl<'a, CtxFactory, CtxT, Query, Mutation> Service for Apollo<'a, CtxFactory, CtxT, Query, Mutation>
//  where
//      CtxFactory: Fn(&mut Request<Body>) -> CtxT + Send + Sync + 'static,
//      CtxT: 'static,
//      Query: GraphQLType<Context=CtxT, TypeInfo=()> + Send + Sync + 'static,
//      Mutation: GraphQLType<Context=CtxT, TypeInfo=()> + Send + Sync + 'static,
//{
//  type ReqBody = Body;
//  type ResBody = Body;
//  type Error = hyper::Error;
//  type Future = Box<Future<Item=Response<Self::ResBody>, Error=Self::Error> + Send>;
//
//  fn call(&mut self, req: Request<Body>) -> Self::Future {
//    let (parts, body) = req.into_parts();
//    let mut response = Response::new(Body::empty());
//
//    match (&parts.method, parts.uri.path()) {
//      (&Method::GET, "/") => {
//        *response.body_mut() = Body::from(playground("/graphql").into());
//      }
//      (&Method::POST, "/graphql") => {
//        body.concat2().and_then(|b| {
//          let req = serde_json::from_slice::<GraphQLRequest>(b.as_ref())?;
//          let context = (self.context_factory)(&parts);
//          let res = req.execute(&self.root_node, context);
//        });
//        // we'll be back
//      }
//      _ => {
//        *response.status_mut() = StatusCode::NOT_FOUND;
//      }
//    };
//
//    Box::new(future::ok(response))
//  }
//}

fn playground(graphql_endpoint_url: &str) -> String {
  // separate stylesheet for the curlies

  let stylesheet_source = r#"
    <style>
      body {
        background-color: rgb(23, 42, 58);
        font-family: Open Sans, sans-serif;
        height: 90vh;
      }
      .loading {
        font-size: 32px;
        font-weight: 200;
        color: rgba(255, 255, 255, .6);
        margin-left: 20px;
      }
      img {
        width: 78px;
        height: 78px;
      }
      .title {
        font-weight: 400;
      }
    </style>
    "#;

  format!(r#"
<!DOCTYPE html>
<html>

<head>
  <meta charset=utf-8/>
  <meta name="viewport" content="user-scalable=no, initial-scale=1.0, minimum-scale=1.0, maximum-scale=1.0, minimal-ui">
  <title>GraphQL Playground</title>
  {stylesheet_source}
  <link rel="stylesheet" href="//cdn.jsdelivr.net/npm/graphql-playground-react/build/static/css/index.css" />
  <link rel="shortcut icon" href="//cdn.jsdelivr.net/npm/graphql-playground-react/build/favicon.png" />
  <script src="//cdn.jsdelivr.net/npm/graphql-playground-react/build/static/js/middleware.js"></script>
</head>

<body>
  <div id="root">
    <img src='//cdn.jsdelivr.net/npm/graphql-playground-react/build/logo.png' alt=''>
    <div class="loading"> Loading
      <span class="title">GraphQL Playground</span>
    </div>
  </div>
  <script>window.addEventListener('load', function (event) {{
      GraphQLPlayground.init(document.getElementById('root'), {{
        // options as 'endpoint' belong here
        endpoint: '{graphql_url}'
      }})
    }})</script>
</body>

</html>
"#,
          graphql_url = graphql_endpoint_url,
          stylesheet_source = stylesheet_source)
}
