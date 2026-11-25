# 3dbrowser 素材下载地址

模块产物（browser3d.js / browser3d_bg.wasm / index.js）构建配方见
assets/modules/3dbrowser/Cargo.toml 头注释。以下为模型素材来源，
全部 CC0（Khronos glTF-Sample-Assets）：

| 文件 | 下载地址 |
|------|----------|
| MaterialsVariantsShoe.glb | https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/main/Models/MaterialsVariantsShoe/glTF-Binary/MaterialsVariantsShoe.glb |
| ToyCar.glb | https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/main/Models/ToyCar/glTF-Binary/ToyCar.glb |

容器镜像构建按此表自动下载（Dockerfile：`ARG GLTF_SAMPLE_ASSETS_COMMIT` 钉住 commit，
两个 sha256 写在那条 RUN 里做校验），并在镜像内构建模块产物（prebuilt
wasm-bindgen 0.2.129，与 assets/modules/3dbrowser/Cargo.lock 一致）——
`trunk build` 会把 assets/ 整个拷进 dist，所以这两样必须在它之前落盘。

许可：各模型 README 见 https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/<名称>
（均为 Creative Commons Zero / Public Domain，属主见各 README credits 行）。