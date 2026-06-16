# 最小 POSIX 子集

本文件定义 Zeronix 课程设计要实现的最小 POSIX.1 子集。目标是用最小的接口支撑一个可工作的 Unix-like shell 和基础工具。

> 参考来源：POSIX.1-2008 / Single UNIX Specification、Linux x86_64 syscall table、xv6。

---

## 设计原则

1. **够用即可**：不追求完整 POSIX 合规，只覆盖 shell 和用户工具所需。
2. **syscall 与 libc 分离**：内核只实现系统调用；C 库负责封装和 POSIX 语义（如 `errno`、缓冲）。
3. **错误处理**：系统调用返回 `-errno` 或设置 `errno`，与 Linux 兼容。
4. **简化版本**：某些调用可以先做简化语义（例如 `fcntl` 只支持 `F_DUPFD`）。

---

## 进程控制

| 调用 | 说明 | 优先级 |
|---|---|---|
| `fork` | 创建子进程 | 高 |
| `vfork` | 共享地址空间的 fork（可简化） | 低 |
| `exit` / `_exit` | 终止进程并返回状态 | 高 |
| `wait` | 等待任一子进程结束 | 高 |
| `waitpid` | 等待指定子进程，支持 `WNOHANG` | 高 |
| `execve` | 加载新程序替换当前进程 | 高 |
| `getpid` | 获取当前进程 ID | 高 |
| `getppid` | 获取父进程 ID | 中 |
| `getuid` / `getgid` | 获取用户/组 ID（可恒返回 0） | 低 |
| `setuid` / `setgid` | 设置用户/组 ID（可简化） | 低 |
| `getpgrp` / `setsid` | 进程组 / 会话 | 低 |

---

## 文件 I/O

| 调用 | 说明 | 优先级 |
|---|---|---|
| `open` | 打开/创建文件 | 高 |
| `close` | 关闭文件描述符 | 高 |
| `read` | 读文件 | 高 |
| `write` | 写文件 | 高 |
| `lseek` | 移动文件偏移 | 高 |
| `dup` | 复制 fd 到最小可用号 | 高 |
| `dup2` | 复制 fd 到指定号 | 高 |
| `fcntl` | 仅实现 `F_DUPFD`、`F_GETFL`、`F_SETFL` | 中 |
| `access` | 检查文件访问权限 | 中 |
| `stat` / `fstat` / `lstat` | 获取文件元信息 | 高 |
| `unlink` | 删除文件 | 中 |
| `rename` | 重命名文件 | 中 |
| `ioctl` | 设备控制（最小实现） | 中 |

### 文件打开标志（`open`）

```c
O_RDONLY    0x0000
O_WRONLY    0x0001
O_RDWR      0x0002
O_CREAT     0x0040
O_EXCL      0x0080
O_NOCTTY    0x0100
O_TRUNC     0x0200
O_APPEND    0x0400
O_NONBLOCK  0x0800
O_DIRECTORY 0x10000
```

---

## 内存管理

| 调用 | 说明 | 优先级 |
|---|---|---|
| `brk` / `sbrk` | 调整数据段末尾 | 高 |
| `mmap` | 匿名映射或文件映射 | 中 |
| `munmap` | 取消映射 | 中 |

### `mmap` 简化语义

- 必须支持 `MAP_ANONYMOUS | MAP_PRIVATE` 用于 libc 堆扩展
- 可选支持文件映射
- `PROT_READ` / `PROT_WRITE` / `PROT_EXEC`
- `MAP_FIXED` 可暂不实现

---

## 目录与文件系统路径

| 调用 | 说明 | 优先级 |
|---|---|---|
| `chdir` | 改变当前工作目录 | 高 |
| `getcwd` | 获取当前工作目录 | 高 |
| `mkdir` | 创建目录 | 中 |
| `rmdir` | 删除目录 | 中 |
| `getdents` / `getdents64` | 读取目录项（libc 的 `readdir` 依赖） | 中 |

---

## 进程间通信

| 调用 | 说明 | 优先级 |
|---|---|---|
| `pipe` / `pipe2` | 创建匿名管道 | 高 |

> 命名管道 `mkfifo` 可作为加分项。

---

## 信号（简化版）

| 调用 | 说明 | 优先级 |
|---|---|---|
| `signal` | 传统信号处理（可兼容实现） | 中 |
| `sigaction` | 可靠信号处理 | 中 |
| `kill` | 向进程发送信号 | 中 |
| `raise` | 向自身发送信号 | 低 |
| `alarm` | 定时信号 SIGALRM | 低 |

### 至少支持的信号

```c
SIGHUP   1
SIGINT   2   // Ctrl+C
SIGQUIT  3
SIGILL   4
SIGABRT  6
SIGFPE   8
SIGKILL  9
SIGSEGV  11
SIGPIPE  13
SIGALRM  14
SIGTERM  15
SIGCHLD  17
SIGCONT  18
SIGSTOP  19
SIGTSTP  20
```

---

## 终端与 I/O 控制

| 调用 | 说明 | 优先级 |
|---|---|---|
| `tcgetattr` | 获取终端属性 | 中 |
| `tcsetattr` | 设置终端属性 | 中 |
| `isatty` | 判断 fd 是否为终端 | 中 |

### `termios` 简化

- 仅支持 `ECHO`、`ICANON`（行缓冲）、`VMIN`、`VTIME`
- 非规范模式可作为加分项

---

## 时间与睡眠

| 调用 | 说明 | 优先级 |
|---|---|---|
| `gettimeofday` | 获取 wall-clock 时间 | 中 |
| `time` | 获取秒级时间 | 中 |
| `nanosleep` | 纳秒级睡眠 | 中 |
| `clock_gettime` | 可选 | 低 |

---

## 杂项

| 调用 | 说明 | 优先级 |
|---|---|---|
| `errno` | 全局错误码 | 高 |
| `getdents64` | 目录读取 | 中 |
| `arch_prctl` | x86_64 特定（FS/GS base） | 低 |
| `reboot` | 重启/关机 | 低 |

---

## 错误码（errno）

参考 Linux 定义，至少实现以下：

```c
#define EPERM        1   /* Operation not permitted */
#define ENOENT       2   /* No such file or directory */
#define ESRCH        3   /* No such process */
#define EINTR        4   /* Interrupted system call */
#define EIO          5   /* I/O error */
#define ENXIO        6   /* No such device or address */
#define E2BIG        7   /* Argument list too long */
#define ENOEXEC      8   /* Exec format error */
#define EBADF        9   /* Bad file number */
#define ECHILD      10   /* No child processes */
#define EAGAIN      11   /* Try again */
#define ENOMEM      12   /* Out of memory */
#define EACCES      13   /* Permission denied */
#define EFAULT      14   /* Bad address */
#define EBUSY       16   /* Device or resource busy */
#define EEXIST      17   /* File exists */
#define EXDEV       18   /* Cross-device link */
#define ENODEV      19   /* No such device */
#define ENOTDIR     20   /* Not a directory */
#define EISDIR      21   /* Is a directory */
#define EINVAL      22   /* Invalid argument */
#define ENFILE      23   /* File table overflow */
#define EMFILE      24   /* Too many open files */
#define ENOTTY      25   /* Not a typewriter */
#define EFBIG       27   /* File too large */
#define ENOSPC      28   /* No space left on device */
#define ESPIPE      29   /* Illegal seek */
#define EROFS       30   /* Read-only file system */
#define EMLINK      31   /* Too many links */
#define EPIPE       32   /* Broken pipe */
#define ERANGE      34   /* Math result not representable */
#define ENAMETOOLONG 36  /* File name too long */
#define ENOSYS      38   /* Function not implemented */
```

---

## 最小 libc 需要封装的函数

如果移植 newlib，只需要实现底层 stub；如果自写 libc，至少需要：

```c
// 标准 I/O
int printf(const char *fmt, ...);
int sprintf(char *buf, const char *fmt, ...);
int putchar(int c);
int puts(const char *s);
int getchar(void);

// 字符串/内存
void *malloc(size_t size);
void free(void *ptr);
void *calloc(size_t n, size_t size);
void *realloc(void *ptr, size_t size);
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *s, int c, size_t n);
int strcmp(const char *a, const char *b);
size_t strlen(const char *s);
char *strcpy(char *dst, const char *src);
char *strncpy(char *dst, const char *src, size_t n);

// 进程
pid_t fork(void);
int execve(const char *path, char *const argv[], char *const envp[]);
void _exit(int status);
pid_t wait(int *status);
pid_t waitpid(pid_t pid, int *status, int options);

// 文件 I/O
int open(const char *path, int flags, ...);
int close(int fd);
ssize_t read(int fd, void *buf, size_t count);
ssize_t write(int fd, const void *buf, size_t count);
off_t lseek(int fd, off_t offset, int whence);
int dup(int fd);
int dup2(int fd, int fd2);
int stat(const char *path, struct stat *buf);

// 目录
int chdir(const char *path);
char *getcwd(char *buf, size_t size);
int mkdir(const char *path, mode_t mode);
```

---

## 验收标准

完成以下场景即视为最小 POSIX 子集实现成功：

1. 启动后进入用户态 shell。
2. 在 shell 中可执行 `/bin/echo hello world`。
3. 支持管道：`/bin/ls | /bin/wc -l`。
4. 支持重定向：`/bin/echo hi > /tmp/a.txt && /bin/cat /tmp/a.txt`。
5. 支持前台进程等待与 `Ctrl+C`（SIGINT）。
6. 支持 `cd` / `pwd` 改变和显示当前目录。
7. 用户程序能使用 `malloc` / `free` 并通过 `brk`/`mmap` 动态扩展内存。
