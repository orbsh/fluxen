use super::Accrete;
use crate::AccreteOps;
use minijinja::Environment;

impl Accrete {
    pub fn expand(&mut self, env: &Environment) {
        if let Accrete::template(r) = self {
            let n = &r.name;
            let cx = &r.data;
            let n = env
                .get_template(n)
                .map_err(|e| e.to_string())
                .and_then(|t| {
                    t.render(cx)
                        .map_err(|e| format!("render failed: {} => {:#?}", e, &cx))
                })
                .and_then(|t| {
                    serde_json::from_str::<Accrete>(&t)
                        .map_err(|e| format!("deserialize failed: {} => {}", e, &t))
                });
            match n {
                Ok(x) => {
                    *self = x;
                }
                Err(_) => {}
            }
        }
        if let Some(cs) = self.borrow_children_mut() {
            for c in cs {
                c.expand(env);
            }
        }
    }
}
