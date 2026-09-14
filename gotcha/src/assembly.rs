//! Assemble application endpoints before applying application-wide middleware.

use axum::Router;

use crate::{GotchaResult, GotchaRouter};

pub(crate) fn assemble<State: Clone + Send + Sync + 'static>(
    router: GotchaRouter<State>, state: State, #[cfg(feature = "openapi")] endpoints: Option<crate::OpenApiEndpoints>, finish: impl FnOnce(Router) -> Router,
) -> GotchaResult<Router> {
    #[cfg(feature = "openapi")]
    let router = if let Some(endpoints) = endpoints {
        endpoints.validate()?;
        let (router, spec) = router.into_openapi_parts();
        endpoints.mount(router.with_state(state), spec)
    } else {
        router.into_axum_router(state)
    };
    #[cfg(not(feature = "openapi"))]
    let router = router.into_axum_router(state);

    Ok(finish(router))
}

#[cfg(test)]
mod tests;
