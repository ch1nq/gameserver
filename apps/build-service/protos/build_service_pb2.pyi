from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class BuildRequest(_message.Message):
    __slots__ = ("name", "git_repo", "dockerfile_path", "context_sub_path")
    NAME_FIELD_NUMBER: _ClassVar[int]
    GIT_REPO_FIELD_NUMBER: _ClassVar[int]
    DOCKERFILE_PATH_FIELD_NUMBER: _ClassVar[int]
    CONTEXT_SUB_PATH_FIELD_NUMBER: _ClassVar[int]
    name: str
    git_repo: str
    dockerfile_path: str
    context_sub_path: str
    def __init__(self, name: _Optional[str] = ..., git_repo: _Optional[str] = ..., dockerfile_path: _Optional[str] = ..., context_sub_path: _Optional[str] = ...) -> None: ...

class BuildResponse(_message.Message):
    __slots__ = ("status", "message", "build_id")
    class Status(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        SUCCESS: _ClassVar[BuildResponse.Status]
        ERROR: _ClassVar[BuildResponse.Status]
    SUCCESS: BuildResponse.Status
    ERROR: BuildResponse.Status
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    BUILD_ID_FIELD_NUMBER: _ClassVar[int]
    status: BuildResponse.Status
    message: str
    build_id: str
    def __init__(self, status: _Optional[_Union[BuildResponse.Status, str]] = ..., message: _Optional[str] = ..., build_id: _Optional[str] = ...) -> None: ...

class PollBuildRequest(_message.Message):
    __slots__ = ("build_id",)
    BUILD_ID_FIELD_NUMBER: _ClassVar[int]
    build_id: str
    def __init__(self, build_id: _Optional[str] = ...) -> None: ...

class PollBuildResponse(_message.Message):
    __slots__ = ("status", "message", "build_status")
    class Status(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        SUCCESS: _ClassVar[PollBuildResponse.Status]
        ERROR: _ClassVar[PollBuildResponse.Status]
    SUCCESS: PollBuildResponse.Status
    ERROR: PollBuildResponse.Status
    class BuildStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        UNKNOWN: _ClassVar[PollBuildResponse.BuildStatus]
        RUNNING: _ClassVar[PollBuildResponse.BuildStatus]
        SUCCEEDED: _ClassVar[PollBuildResponse.BuildStatus]
        FAILED: _ClassVar[PollBuildResponse.BuildStatus]
    UNKNOWN: PollBuildResponse.BuildStatus
    RUNNING: PollBuildResponse.BuildStatus
    SUCCEEDED: PollBuildResponse.BuildStatus
    FAILED: PollBuildResponse.BuildStatus
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    BUILD_STATUS_FIELD_NUMBER: _ClassVar[int]
    status: PollBuildResponse.Status
    message: str
    build_status: PollBuildResponse.BuildStatus
    def __init__(self, status: _Optional[_Union[PollBuildResponse.Status, str]] = ..., message: _Optional[str] = ..., build_status: _Optional[_Union[PollBuildResponse.BuildStatus, str]] = ...) -> None: ...

class DeployRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class DeployResponse(_message.Message):
    __slots__ = ("status", "message")
    class Status(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        SUCCESS: _ClassVar[DeployResponse.Status]
        ERROR: _ClassVar[DeployResponse.Status]
    SUCCESS: DeployResponse.Status
    ERROR: DeployResponse.Status
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    status: DeployResponse.Status
    message: str
    def __init__(self, status: _Optional[_Union[DeployResponse.Status, str]] = ..., message: _Optional[str] = ...) -> None: ...
