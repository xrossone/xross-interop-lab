# Task Report

填写真实task ID、implementation commit、日期、修改文件及产生的public interfaces。

## Evidence

每条实际命令记录环境、exit code、输出/日志hash；设备测试记录型号/OS/网络；没有执行的项目写not-run。保留失败case与解决它的后续run，不改写旧结果。

## Result

按planned/source-reviewed/build-verified/simulated/device-verified/release-qualified/blocked描述范围。列通过、失败、已知限制；不能用一句“完成”覆盖多个不同成熟度。

## Provenance and dependencies

列引用资料、读取过的restricted资料、新依赖/feature、二进制/SDK许可影响。

## Next boundary

写不受影响可以继续的下一个task、需要契约裁决的变更、必须先解决的阻塞。不能为通过CI而删安全测试或降低质量阈值。
