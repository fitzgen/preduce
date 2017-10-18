//! `preduce`'s IPC protocol request and response type definitions.
//!
//! All requests and responses are serialized as JSON.
//!
//! Each request and response is serialized on a single line, followed by a
//! newline.

#![deny(missing_docs)]

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use std::path::PathBuf;

/// A request from `preduce` to a reducer script.
///
/// The reducer script's response must match the request:
///
/// * `Request::New` must be responded to with `Response::New`
/// * `Request::Next` must be responded to with `Response::Next`
/// * Etc...
///
/// The exception is `Request::Shutdown`. Reducer scripts must exit without
/// responding upon receipt of `Request::Shutdown`.
///
/// The `state` JSON values will only ever be JSON values returned from previous
/// requests.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum Request {
    /// `{ "Shutdown": null }`
    Shutdown,

    /// `{ "New": { "seed": "path/to/seed" } }`
    New(NewRequest),

    /// `{ "Next": { "seed": "path/to/seed", "state": <JSON value> } }`
    Next(NextRequest),

    /// `{ "Next": { "old_seed": "path/to/old/seed", "new_seed": "path/to/new/seed", "state": <JSON value> } }`
    NextOnInteresting(NextOnInterestingRequest),

    /// `{ "FastForward": { "seed": "path/to/seed", "state": <JSON value>, "n": <unsigned integer> } }`
    FastForward(FastForwardRequest),

    /// `{ "Reduce": { "seed": "path/to/seed", "state": <JSON value>, "dest": "path/to/dest" } }`
    Reduce(ReduceRequest),
}

/// Construct a new reducer state.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NewRequest {
    /// The known-interesting seed test case the new reducer state should be
    /// created from.
    pub seed: PathBuf,
}

/// Get the next state.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextRequest {
    /// The known-interesting seed test case that the given state was created
    /// for.
    pub seed: PathBuf,
    /// The current state.
    pub state: serde_json::Value,
}

/// Get the next state when it is known that the given state produced a new
/// interesting test case from the old test case.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextOnInterestingRequest {
    /// The known-interesting seed test case that the given state was created
    /// for.
    pub old_seed: PathBuf,
    /// The new known-interesting test case that we previously generated with
    /// the given state in a `Request::Reduce` request.
    pub new_seed: PathBuf,
    /// The current state.
    pub state: serde_json::Value,
}

/// Get the n^th state after the given state for the given known-interesting
/// seed test case.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct FastForwardRequest {
    /// The known-interesting seed test case that the given state was created
    /// for.
    pub seed: PathBuf,
    /// The number of states to skip forward.
    pub n: usize,
    /// The current state.
    pub state: serde_json::Value,
}

/// Create a reduction of the given known-interesting seed test case at the
/// given destination path and with the given reduction state.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ReduceRequest {
    /// The known-interesting seed test case that the given state was created
    /// for.
    pub seed: PathBuf,
    /// The current state.
    pub state: serde_json::Value,
    /// The path where the reduction should be created at.
    pub dest: PathBuf,
}

/// A response from a reducer script to a `Request` sent by `preduce`.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum Response {
    /// `{ "New": { "state": <JSON value> } }`
    New(NewResponse),

    /// `{ "Next": { "next_state": <JSON value> } }`
    Next(NextResponse),

    /// `{ "NextOnInteresting": { "next_state": <JSON value> } }`
    NextOnInteresting(NextOnInterestingResponse),

    /// `{ "FastForward": { "next_state": <JSON value> } }`
    FastForward(FastForwardResponse),

    /// `{ "Reduce": { "reduced": <bool> } }`
    Reduce(ReduceResponse),
}

/// A response to `Request::New` with the new reduction state.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NewResponse {
    /// The new state.
    pub state: serde_json::Value,
}

/// A response to `Request::Next` with the next reduction state.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextResponse {
    /// The next state.
    pub next_state: Option<serde_json::Value>,
}

/// A response to `Request::NextOnInteresting` with the next reduction state.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextOnInterestingResponse {
    /// The next state.
    pub next_state: Option<serde_json::Value>,
}

/// A response to `Request::FastForward` with the next reduction state.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct FastForwardResponse {
    /// The next state.
    pub next_state: Option<serde_json::Value>,
}

/// A response to `Request::Reduce`.
///
/// If the reduction was generated into the requested destination path, then
/// `reduced` is `true`. If not, then `reduced` is `false`.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ReduceResponse {
    /// Whether a potential reduction was created at the request's destination
    /// path.
    pub reduced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use std::path::PathBuf;

    #[test]
    fn request_is_serializable_deserializable() {
        let request = Request::New(NewRequest {
            seed: PathBuf::from("/path/to/seed"),
        });

        let serialized = serde_json::to_string(&request).unwrap();
        println!("serialized to `{}`", serialized);

        let deserialized = serde_json::from_str(&serialized).unwrap();
        println!("deserialized to {:?}", deserialized);

        assert_eq!(request, deserialized);
    }

    #[test]
    fn response_is_serializable_deserializable() {
        let response = Response::New(NewResponse {
            state: serde_json::to_value((120, 0)).unwrap(),
        });

        let serialized = serde_json::to_string(&response).unwrap();
        println!("serialized to `{}`", serialized);

        let deserialized = serde_json::from_str(&serialized).unwrap();
        println!("deserialized to {:?}", deserialized);

        assert_eq!(response, deserialized);
    }
}
