use gotcha::Json;
use gotcha::api;

struct CanSkipFields;

#[api]
pub async fn handler_with_can_skip_fields(#[api(skip)] can_skip_fields: CanSkipFields) -> () {
    ()
}

fn main(){}