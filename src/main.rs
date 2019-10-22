extern crate regex;
extern crate futures;
extern crate hyper;

extern crate toml;
#[macro_use]
extern crate serde_derive;
extern crate s3;

extern crate url;

use std::process;
use std::env;
use std::error::Error;

use futures::future;
use hyper::{Body, Method, Response, Request, Server, StatusCode};
use hyper::rt::Future;
use hyper::service::service_fn;
use hyper::header::{HeaderMap, LOCATION};

//use url::Url;

type BoxFut = Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send>;

mod endpoint;

fn get_config() -> Result<endpoint::Endpoint, Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    let region = args[2].clone();
    let endpoints = endpoint::from_file(args[1].clone())?;
    if !endpoints.contains_key(&region) {
        return Err(From::from(format!("config toml doesn't contain region {}", args[2])));
    }

    return Ok(endpoints[&region].clone());
}

fn router(req: Request<Body>) -> BoxFut {
    let mut response = Response::new(Body::empty());

    let endpoint = match get_config() {
        Ok(o) => o,
        Err(e) => {
            println!("Error: {}", e);
            process::exit(1);
        }
    };
    let inventory = match endpoint::process_s3(&endpoint) {
        Ok(o) => o,
        Err(e) => {
			println!("Error: {}", e);
			process::exit(1);
		}
    };
    let architectures: Vec<String> = inventory.iter()
        .map(|i| i.prefix.clone().replace("/", ""))
        .collect();

    match(req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            *response.body_mut() = Body::from(architectures.join("<br/>"));
        }

        (&Method::GET, _) => {
            let req_uri = req.uri().path().to_string();
            let req_parts: Vec<&str> = req_uri.split("/").filter(|v| v != &"").collect();
            if !architectures.contains(&req_parts.first().unwrap().to_string()) {
                *response.status_mut() = StatusCode::NOT_FOUND;
            } else {
                let base_pub_uri = format!("{}/{}", &endpoint.public_url, &endpoint.s3_bucket.unwrap());
                let mut final_url = String::new();
                if req_parts.last().unwrap() == &"current" {
                    //final_url = format!("{}{}", base_pub_uri, inventory.latest.file);
                    final_url = format!("{}/CATS", base_pub_uri);
                } else {
                    final_url = format!("{}{}", base_pub_uri, req_uri);
                }
                let mut headers = HeaderMap::new();
                headers.insert(LOCATION, final_url.as_str().parse().unwrap());
                *response.headers_mut() = headers;
                *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
            }
        }

        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };
    Box::new(future::ok(response))
}

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 3 {
        println!("Usage: {} <config_toml> <region>", args[0]);
        process::exit(1);
    }

    let config = match (get_config()) {
        Ok(c) => c,
        Err(e) => {
            println!("Error: {}", e);
            process::exit(1);
        },
    };

    let addr = ([0, 0, 0, 0], 8080).into();
    let server = Server::bind(&addr)
        .serve(|| service_fn(router))
        .map_err(|e| println!("server error: {}", e));

    hyper::rt::run(server);
}
