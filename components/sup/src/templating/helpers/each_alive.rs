// Copyright (c) 2016-2017 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::BTreeMap;

use handlebars::{Handlebars, Helper, HelperDef, Renderable, RenderContext, RenderError};
use serde_json::Value as Json;

use super::super::RenderResult;
use super::{to_json, JsonTruthy};

#[derive(Clone, Copy)]
pub struct EachAliveHelper;

impl HelperDef for EachAliveHelper {
    fn call(&self, h: &Helper, r: &Handlebars, rc: &mut RenderContext) -> RenderResult<()> {
        let value = h.param(0).ok_or_else(|| {
            RenderError::new("Param not found for helper \"eachAlive\"")
        })?;
        if let Some(template) = h.template() {
            rc.promote_local_vars();
            let local_path_root = value.path_root().map(
                |p| format!("{}/{}", rc.get_path(), p),
            );
            let rendered = match (value.value().is_truthy(), value.value()) {
                (true, &Json::Array(ref list)) => {
                    let len = list.len();
                    for i in 0..len {
                        let member = list[i].as_object().ok_or_else(|| {
                            RenderError::new(format!(
                                "Param value is not a valid census \
                                member. Parameter content is: {:?}",
                                list[i]
                            ))
                        })?;
                        if member.contains_key("alive") && member["alive"].as_bool().unwrap() {
                            let mut local_rc = rc.derive();
                            local_rc.set_local_var("@first".to_string(), to_json(&(i == 0usize)));
                            local_rc.set_local_var("@last".to_string(), to_json(&(i == len - 1)));
                            local_rc.set_local_var("@index".to_string(), to_json(&i));

                            if let Some(block_param) = h.block_param() {
                                let mut map = BTreeMap::new();
                                map.insert(block_param.to_string(), to_json(&list[i]));
                                local_rc.push_block_context(&map);
                            }

                            template.render(r, &mut local_rc)?;

                            if h.block_param().is_some() {
                                local_rc.pop_block_context();
                            }
                        }
                    }
                    Ok(())
                }
                (true, &Json::Object(ref obj)) => {
                    let mut first: bool = true;
                    if !obj.contains_key("alive") || !obj["alive"].as_bool().unwrap() {
                        return Ok(());
                    }
                    for k in obj.keys() {
                        let mut local_rc = rc.derive();
                        if let Some(ref p) = local_path_root {
                            local_rc.push_local_path_root(p.clone());
                        }
                        local_rc.set_local_var("@first".to_string(), to_json(&first));
                        local_rc.set_local_var("@key".to_string(), to_json(k));

                        if first {
                            first = false;
                        }

                        if let Some(inner_path) = value.path() {
                            let new_path =
                                format!("{}/{}.[{}]", local_rc.get_path(), inner_path, k);
                            local_rc.set_path(new_path);
                        }

                        if let Some((bp_key, bp_val)) = h.block_param_pair() {
                            let mut map = BTreeMap::new();
                            map.insert(bp_key.to_string(), to_json(k));
                            map.insert(bp_val.to_string(), to_json(obj.get(k).unwrap()));
                            local_rc.push_block_context(&map);
                        }

                        template.render(r, &mut local_rc)?;

                        if h.block_param().is_some() {
                            local_rc.pop_block_context();
                        }

                        if local_path_root.is_some() {
                            local_rc.pop_local_path_root();
                        }
                    }
                    Ok(())
                }
                (false, _) => {
                    if let Some(else_template) = h.inverse() {
                        else_template.render(r, rc)?;
                    }
                    Ok(())
                }
                _ => Err(RenderError::new(
                    format!("Param type is not iterable: {:?}", template),
                )),
            };

            rc.demote_local_vars();
            return rendered;
        }
        Ok(())
    }
}

pub static EACH_ALIVE: EachAliveHelper = EachAliveHelper;
