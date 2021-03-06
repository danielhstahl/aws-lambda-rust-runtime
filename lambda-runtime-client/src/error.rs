//! This module defines the `RuntimeApiError` trait that developers should implement
//! to send their custom errors to the AWS Lambda Runtime Client SDK. The module also
//! defines the `ApiError` type returned by the `RuntimeClient` implementations.
use failure::{AsFail, Backtrace, Context, Fail};
use lambda_runtime_errors::LambdaErrorExt;
use log::*;
use serde_derive::*;
use std::{
    fmt::{self, Display},
    option::Option,
};

/// Error type for the error responses to the Runtime APIs. In the future, this library
/// should use a customer-generated error code
pub const RUNTIME_ERROR_TYPE: &str = "RustRuntimeError";

/// This object is used to generate requests to the Lambda Runtime APIs.
/// It is used for both the error response APIs and fail init calls.
/// custom error types should implement the `RuntimeError` trait and return
/// this object to be compatible with the APIs.
#[derive(Serialize)]
pub struct ErrorResponse {
    /// The error message generated by the application.
    #[serde(rename = "errorMessage")]
    pub error_message: String,
    /// The error type for Lambda. Normally, this value is populated using the
    /// `error_type()` method from the `LambdaErrorExt` trait.
    #[serde(rename = "errorType")]
    pub error_type: String,
    /// The stack trace for the exception as vector of strings. In the framework,
    /// this value is automatically populated using the `backtrace` crate.
    #[serde(rename = "stackTrace")]
    pub stack_trace: Option<Vec<String>>,
}

impl ErrorResponse {
    /// Creates a new instance of the `ErrorResponse` object with the given parameters. If the
    /// `RUST_BACKTRACE` env variable is `1` the `ErrorResponse` is populated with the backtrace
    /// collected through the [`backtrace` craete](https://crates.io/crates/backtrace).
    ///
    /// # Arguments
    ///
    /// * `message` The error message to be returned to the APIs. Normally the error description()
    /// * `err_type` An error type that identifies the root cause. Normally populated by the
    ///   `error_type()` method in the `LambdaErrorExt` trait.
    /// * `backtrace` The stack trace for the error
    ///
    /// # Return
    /// A new instance of the `ErrorResponse` object.
    fn new(message: String, err_type: String, backtrace: Option<&Backtrace>) -> Self {
        let mut err = ErrorResponse {
            error_message: message,
            error_type: err_type,
            stack_trace: Option::default(),
        };
        // assume that failure is smart enough to only collect a backtrace
        // if the env variable is enabled
        if let Some(stack) = backtrace {
            trace!("Begin backtrace collection");
            err.stack_trace = Some(
                format!("{:?}", stack)
                    .lines()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<String>>(),
            );
            trace!("Completed backtrace collection");
        }

        err
    }
}

impl<T: AsFail + LambdaErrorExt + Display> From<T> for ErrorResponse {
    fn from(e: T) -> Self {
        ErrorResponse::new(format!("{}", e), e.error_type().to_owned(), e.as_fail().backtrace())
    }
}

/// Represents an error generated by the Lambda Runtime API client.
#[derive(Debug)]
pub struct ApiError {
    inner: Context<ApiErrorKind>,
}

impl ApiError {
    /// Returns `true` if the API error is recoverable and should be retried
    pub fn is_recoverable(&self) -> bool {
        match *self.inner.get_context() {
            ApiErrorKind::Recoverable(_) => true,
            _ => false,
        }
    }
}
/// Failure context for the `ApiError` type. The kind is used to indicate whether the
/// error is recoverable and should be retried or not.
#[derive(Clone, PartialEq, Debug, Fail)]
pub enum ApiErrorKind {
    /// Runtime implementations that receive recoverable errors should automatically
    /// retry requests
    #[fail(display = "Recoverable API error: {}", _0)]
    Recoverable(String),
    /// Unrecoverable error should cause the runtime implementation to call the `fail_init`
    /// method of the Runtime APIs if it is appropriate and then shutdown gracefully
    #[fail(display = "Unrecoverable API error: {}", _0)]
    Unrecoverable(String),
}

impl Fail for ApiError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl LambdaErrorExt for ApiError {
    fn error_type(&self) -> &str {
        "RuntimeApiError"
    }
}

impl From<ApiErrorKind> for ApiError {
    fn from(kind: ApiErrorKind) -> Self {
        Self {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ApiErrorKind>> for ApiError {
    fn from(inner: Context<ApiErrorKind>) -> Self {
        Self { inner }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use failure::format_err;
    use std::env;

    #[test]
    fn does_not_produce_stack_trace() {
        env::remove_var("RUST_BACKTRACE");
        let err = format_err!("Test error").compat();
        let resp_err = ErrorResponse::from(err);
        assert_eq!(resp_err.stack_trace, None);
    }

    #[test]
    fn is_recoverable_eq_correctly() {
        let rec_err = ApiError::from(ApiErrorKind::Recoverable("Some recoverable kind".to_owned()));
        assert_eq!(true, rec_err.is_recoverable());
        let unrec_err = ApiError::from(ApiErrorKind::Unrecoverable("Some unrecovrable kind".to_owned()));
        assert_eq!(false, unrec_err.is_recoverable());
    }
}
