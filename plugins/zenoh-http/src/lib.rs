//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
#![feature(async_closure)]

use futures::prelude::*;
use clap::{Arg, ArgMatches};
use zenoh::net::*;
use zenoh_router::runtime::Runtime;
use tide::{Request, Response, Server, StatusCode};
use tide::http::Mime;
use std::str::FromStr;

const PORT_SEPARATOR: char = ':';
const DEFAULT_HTTP_HOST: &str = "0.0.0.0";
const DEFAULT_HTTP_PORT: &str = "8000";

fn parse_http_port(arg: &str) -> String {
    match arg.split(':').count() {
        1 => {
            match arg.parse::<u16>() {
                Ok(_) => {[DEFAULT_HTTP_HOST, arg].join(&PORT_SEPARATOR.to_string())} // port only
                Err(_) => {[arg, DEFAULT_HTTP_PORT].join(&PORT_SEPARATOR.to_string())} // host only
            }
        }
        _ => {arg.to_string()}
    }
}

#[no_mangle]
pub fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>>
{
    vec![
        Arg::from_usage("--http-port 'The listening http port'")
        .default_value(DEFAULT_HTTP_PORT)
    ]
}

#[no_mangle]
pub fn start(runtime: Runtime, args: ArgMatches<'static>)
{
    async_std::task::spawn(run(runtime, args));
}

async fn to_json(results: async_std::sync::Receiver<Reply>) -> String{
    let values = results.filter_map(async move |reply| match reply {
        Reply::ReplyData {reskey, payload, ..} => 
            Some(format!("{{ \"key\": \"{}\",\n  \"value\": \"{}\",\n  \"time\": \"{}\" }}",
                        reskey, String::from_utf8_lossy(&payload.to_vec()), "None")), // TODO timestamp
        _ => None,
    }).collect::<Vec<String>>().await.join(",\n");
    format!("[\n{}\n]", values)
}

async fn run(runtime: Runtime, args: ArgMatches<'_>) {
    env_logger::init();

    let http_port = parse_http_port(args.value_of("http-port").unwrap());

    let session = Session::init(runtime).await;

    let mut app = Server::with_state(session);

    app.at("*").get(async move |req: Request<Session>| { 
        let split = req.url().path().split('?').collect::<Vec<&str>>();
        let path = split[0];
        let predicate = match split.len() {
            1 => "",
            _ => split[1],
        };
        match req.state().query(
                &path.into(), &predicate,
                QueryTarget::default(),
                QueryConsolidation::default()).await {
            Ok(stream) => {
                let mut res = Response::new(StatusCode::Ok);
                res.set_content_type(Mime::from_str("text/json").unwrap());
                res.set_body(to_json(stream).await);
                Ok(res)
            }
            Err(e) => {
                let mut res = Response::new(StatusCode::InternalServerError);
                res.set_content_type(Mime::from_str("text").unwrap());
                res.set_body(e.to_string());
                Ok(res)
            }
        }
    });

    if let Err(e) = app.listen(http_port).await {
        log::error!("Unable to start http server : {:?}", e);
    }
}

