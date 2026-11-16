//! Which live plays keep React (docs/r3f.feature R3F27).

use std::path::Path;

/// Whether the edit's own sources (its folder, not node_modules) import React Three Fiber (`raea/r3f`,
/// `@react-three/...`): R3F runs a reconciler of its own on React, which live play's runtime (sdk/react/js/lite) isn't,
/// so such a play keeps React, as RAEA_LIVE_REACT=1 does (docs/r3f.feature R3F27)
pub fn needs_react(dir: &Path) -> bool {
    fn walk(d: &Path, depth: usize) -> bool {
        let Ok(rd) = std::fs::read_dir(d) else { return false };
        rd.flatten().any(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let p = e.path();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                false
            } else if p.is_dir() {
                depth < 4 && walk(&p, depth + 1)
            } else {
                [".tsx", ".ts", ".jsx", ".js"].iter().any(|x| name.ends_with(x))
                    && std::fs::read_to_string(&p).is_ok_and(|t| t.contains("raea/r3f") || t.contains("@react-three/"))
            }
        })
    }
    walk(dir, 0)
}


#[cfg(test)]
mod needs_react_tests {
    #[test]
    fn an_edit_importing_r3f_keeps_react_and_one_whose_packages_do_doesnt() {
        let d = std::env::temp_dir().join(format!("raea-needs-react-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("node_modules/drei")).unwrap();
        std::fs::write(d.join("edit.tsx"), "import { Composition } from \"raea\";").unwrap();
        std::fs::write(d.join("node_modules/drei/index.js"), "import \"@react-three/fiber\";").unwrap();
        assert!(!super::needs_react(&d));
        std::fs::create_dir_all(d.join("src/scene")).unwrap();
        std::fs::write(d.join("src/scene/ball.tsx"), "import { R3F } from \"raea/r3f\";").unwrap();
        assert!(super::needs_react(&d));
        let _ = std::fs::remove_dir_all(&d);
    }
}
