use std::error::Error;
use serde::{Serialize};
use std::rc::Rc;

use crate::{Credentials, api};
use crate::api::search::PullRequest;


#[derive(Serialize, Debug)]
struct UpdateDescriptionRequest<'a> {
  body: &'a str
}

pub async fn update_description(description: String, pr: Rc<PullRequest>, c: &Credentials) -> Result<(), Box<dyn Error>> {
  let client = reqwest::Client::new();
  let body = UpdateDescriptionRequest { body: &description };
  let request = api::base_patch_request(&client, &c, pr.url()).json(&body);
  request.send().await?;
  Ok(())
}