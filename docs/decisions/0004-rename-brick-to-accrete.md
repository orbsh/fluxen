# ADR 0004: brick crate 更名 accrete

状态：已接受
日期：2026-09-26
仓库：fluxen

## 背景

`brick` 一名立项时主要考虑是字母序（b 开头在 crates 文件树置顶），不是语义。
复盘发现隐喻与本体错位：砖假设放置后不动、可无限组合；而本 crate 的
真实语义是封闭词表 + 纯数据 + 流式累积更新（Concat/Replace/Delete，token
追帧进同一行）。排序动机已主动放弃，名字只剩语义评价，改名只剩收益。

候选遍历：glyph（封闭词表/可变数据对，但无累积时间轴）、splat（高斯泼溅，
clone/split/prune 与合并语义同构，语域偏戏谑）、blob（重叠成图对，但
binary large object 在存储圈占用心智）、mosaic+tile 双层（累积合成对，
Mosaic 指整体不指元素，tile 太通用）、acre（accretion 缩写字母最短）。

## 决策

命名 `accrete`（动词：吸积——引力作用下不断吸附生长）。

理由：它是唯一把"不断累积更新"放在名字主位的候选，其余名字都要靠文档
找补。与项目名 fluxen 的世界观分工自洽：fluxen 管场（flux=流，机制），
accrete 管物质的生长（吸积，结果）——两词各占一根轴，互为因果而非互为
近义（field/induction/polar 等机制词与 fluxen 同义重复，全部排除）。

已知代价（接受）：
- 与 concrete 押形（Rust 社区高频词）——文档语境中"具体类型"讨论需自行
  区分，crate 语义（UI 元素集）实际不交叠。
- 动词做集合名词的语法毛边——`parse_kdl_to_accretes` 复数读感生硬，
  函数名保留该形态，接受。
- crates.io 裸名 `accrete` 被行星生成算法库占用——workspace path 依赖
  无关；将来发布用 `fluxen-accrete`。

## 后果

- 全仓机械改名：crate 目录（brick→accrete、brick_macro→accrete_macro）、
  类型（Brick/Accrete、BrickOps→AccreteOps、ClassifyBrick→ClassifyAccrete）、
  属性宏（#[ui_brick]→#[ui_acrete]）、函数（render_brick→render_accrete 等）。
- wire 格式零变化：砖类型值（text/case/rack…）不含 crate 名，serde 标签不动，
  非破坏性改名。
- CSS 颜色名 firebrick 保留（词内包含，改名时显式保护）。
- 历史 ADR（0001/0002/0003）与 README 中的 crate 名引用同步为 accrete
  （"greps stay truthful"既定规则：决策档案的状态/背景文字不改，代码标识
  符引用照改）；wiki 的 fluxora 文档域（flex-ui.md 等）描述 fluxora 时代
  设计，单独更新。
