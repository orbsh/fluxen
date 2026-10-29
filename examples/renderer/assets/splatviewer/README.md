# splatviewer 素材下载地址

模块（index.js，纯 JS 无构建）见 examples/modules/splatviewer/。
泼溅场景 .ksplat 来自 GaussianSplats3D 官方演示数据包：

| 文件 | 下载地址 |
|------|----------|
| truck.ksplat | 本仓 release 镜像：https://github.com/orbsh/fluxen/releases/download/assets/truck.ksplat （sha256 `e8ed0a5cc03fe7d91c5cea7a53c72e0f3a8bdd805157bee24def1ab059b23654`，与官方包内 truck/truck.ksplat 逐字节一致）|
| bonsai.ksplat | https://projects.markkellogg.org/downloads/gaussian_splat_data.zip （解压 bonsai/bonsai.ksplat）|

上游原始包（590 MB，含 truck / bonsai 等多个场景）：
https://projects.markkellogg.org/downloads/gaussian_splat_data.zip （解压 truck/truck.ksplat）

容器镜像直接从上面的 release 取 truck.ksplat（29 MB，sha256 校验）；bonsai 不进镜像。

许可说明：该数据包是 GaussianSplats3D 仓库演示配套数据，场景源自 INRIA
3D Gaussian Splatting 论文公开数据集（Mip-NeRF 360 的 truck / bonsai）。
演示用途随仓库分发；商用前自行核实源场景许可。
