# 本包附带的本地辅助工具

Python **3.10+** 与Git。生成的clone计划是Bash脚本，适用于macOS/Linux、Windows的Git Bash/WSL；不是PowerShell语法。工具仅辅助来源管理，不是安全沙箱、反恶意软件或许可证认证器。

## 1. 检查文档与机器清单

```bash
python tools/validate_pack.py
python -m unittest discover -s tools/tests -v
```

这些测试只验证本包的数据/辅助程序。它们不编译、不运行AirPlay/QuickShare，也不验证真机兼容。

## 2. 只生成clone命令

```bash
python tools/clone_plan.py --root "$HOME/interop-quarantine" > clone-selected.sh
python tools/clone_plan.py --root "$HOME/interop-quarantine" --group miracast --group cast > clone-cast.sh
python tools/clone_plan.py --root "$HOME/interop-quarantine" --all > clone-all-confirmed.sh
```

默认只选`file-core`和`airplay`；`--all`加入large与gated，但仍排除本轮未成功核验入口的候选。`--include-unverified`必须额外显式提供。本清单目前64项中有1项AOSP入口按候选处理。

**不会执行生成的脚本。**先审查命令，再在无开发/个人凭据的隔离环境中运行。脚本只做shallow `git clone --no-checkout --no-recurse-submodules`，禁本地file/ext Git协议与全局Git配置；不checkout、不运行build、不装依赖。已有目录跳过而不pull/update，随后务必核验origin。

它防止简单参数/路径注入，不承诺能防所有Git客户端漏洞。使用更新过的Git和真实隔离。后续checkout也可能受filter/config影响，必须在隔离环境按固定commit执行；不要让它继承用户的全局构建或agent权限。

## 3. 读取已存在的本地仓库并生成lock

```bash
python tools/resolve_lock.py \
  --root "$HOME/interop-quarantine" \
  --output external-lock.local.json
```

记录origin、HEAD完整commit、顶层LICENSE/COPYING/NOTICE候选文件hash；缺仓库明确missing。默认不覆盖已有lock。只读Git对象，不联网、不fetch、不checkout、不执行项目代码。该工具关闭lazy-fetch；如果手动用partial clone而对象尚不存在，则应显式在隔离环境补齐，不能当已审查。

**只检查顶层许可证候选，不检查每个源文件/依赖，`production_approved`始终false。**来源认证、历史作者、SDK、源代码与crate/binary一致性仍需要人工/后续AI审查。工具生成记录不等于取得复用许可。

## 4. 锁定后怎么研究

为当前task生成窄输入列表，先只读关键README/manifest/许可/构建入口，再逐模块研究。必要时按批准步骤运行isolated baseline。查阅`docs/06-research-provenance-and-licensing.md`，不要在主Xross repo里直接批量编译64个项目。
