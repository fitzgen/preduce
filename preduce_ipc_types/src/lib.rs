extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use std::path::PathBuf;

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum Request {
    Shutdown,
    New(NewRequest),
    Next(NextRequest),
    NextOnInteresting(NextOnInterestingRequest),
    FastForward(FastForwardRequest),
    Reduce(ReduceRequest),
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NewRequest {
    pub seed: PathBuf,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextRequest {
    pub seed: PathBuf,
    pub state: serde_json::Value,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextOnInterestingRequest {
    pub old_seed: PathBuf,
    pub new_seed: PathBuf,
    pub state: serde_json::Value,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct FastForwardRequest {
    pub seed: PathBuf,
    pub n: usize,
    pub state: serde_json::Value,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ReduceRequest {
    pub seed: PathBuf,
    pub state: serde_json::Value,
    pub dest: PathBuf,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum Response {
    New(NewResponse),
    Next(NextResponse),
    NextOnInteresting(NextOnInterestingResponse),
    FastForward(FastForwardResponse),
    Reduce(ReduceResponse),
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NewResponse {
    pub state: serde_json::Value,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextResponse {
    pub next_state: Option<serde_json::Value>,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct NextOnInterestingResponse {
    pub next_state: Option<serde_json::Value>,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct FastForwardResponse {
    pub next_state: Option<serde_json::Value>,
}

/// TODO FITZGEN
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ReduceResponse {
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
