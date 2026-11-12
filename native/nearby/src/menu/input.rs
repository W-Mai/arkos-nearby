use serde_json::{Value, json};

#[derive(PartialEq, Eq)]
pub(super) struct Target {
    context: u64,
    route: Value,
}

impl Target {
    pub(super) fn new(view: &Value, context: u64) -> Self {
        let launcher = &view["launcher"];
        Self {
            context,
            route: json!([
                view["page"],
                view["return_to_launcher"],
                view["network_only"],
                view["current_game"],
                launcher["enabled"],
                launcher["tab"],
                launcher["overlay_kind"],
                if launcher["overlay_kind"]
                    .as_str()
                    .is_some_and(|kind| !kind.is_empty())
                {
                    &launcher["key"]
                } else {
                    &Value::Null
                },
                view["library"]["kind"],
                view["library"]["keyboard"],
                launcher["details"]["section"],
                launcher["details"]["expanded"],
                launcher["wifi"]["epoch"],
                launcher["wifi"]["page"],
                launcher["wifi"]["target"],
                launcher["configuration"]["epoch"],
                launcher["saves"]["epoch"],
                launcher["management"]["epoch"],
                launcher["management"]["entry"]
            ]),
        }
    }

    pub(super) fn context(&self) -> u64 {
        self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_and_owners_change_targets_but_feedback_and_focus_do_not() {
        let view = json!({"page":"games","library":{"kind":"favorites","keyboard":false},"launcher":{"enabled":true,"tab":0,"overlay_kind":"","key":"a"}});
        let target = Target::new(&view, 10);
        let mut focused = view.clone();
        focused["selected"] = 3.into();
        focused["launcher"]["key"] = "b".into();
        focused["busy"] = true.into();
        focused["status"] = "Progress".into();
        focused["entries"] = json!([{"name":"Other"}]);
        assert!(target == Target::new(&focused, 10));
        assert!(target != Target::new(&view, 11));
        for changed in [
            json!({"page":"lobby"}),
            json!({"page":"games","launcher":{"overlay_kind":"game"}}),
            json!({"page":"games","library":{"kind":"search","keyboard":true}}),
        ] {
            assert!(target != Target::new(&changed, 10));
        }
    }
}
