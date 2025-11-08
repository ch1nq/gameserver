from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class DeployAgentRequest(_message.Message):
    __slots__ = ("name", "image_url", "agent_id")
    NAME_FIELD_NUMBER: _ClassVar[int]
    IMAGE_URL_FIELD_NUMBER: _ClassVar[int]
    AGENT_ID_FIELD_NUMBER: _ClassVar[int]
    name: str
    image_url: str
    agent_id: int
    def __init__(self, name: _Optional[str] = ..., image_url: _Optional[str] = ..., agent_id: _Optional[int] = ...) -> None: ...

class DeployAgentResponse(_message.Message):
    __slots__ = ("status", "message", "app_name", "deployed_image_url")
    class Status(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        SUCCESS: _ClassVar[DeployAgentResponse.Status]
        ERROR: _ClassVar[DeployAgentResponse.Status]
    SUCCESS: DeployAgentResponse.Status
    ERROR: DeployAgentResponse.Status
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    APP_NAME_FIELD_NUMBER: _ClassVar[int]
    DEPLOYED_IMAGE_URL_FIELD_NUMBER: _ClassVar[int]
    status: DeployAgentResponse.Status
    message: str
    app_name: str
    deployed_image_url: str
    def __init__(self, status: _Optional[_Union[DeployAgentResponse.Status, str]] = ..., message: _Optional[str] = ..., app_name: _Optional[str] = ..., deployed_image_url: _Optional[str] = ...) -> None: ...

class DeleteAgentRequest(_message.Message):
    __slots__ = ("name", "agent_id")
    NAME_FIELD_NUMBER: _ClassVar[int]
    AGENT_ID_FIELD_NUMBER: _ClassVar[int]
    name: str
    agent_id: int
    def __init__(self, name: _Optional[str] = ..., agent_id: _Optional[int] = ...) -> None: ...

class DeleteAgentResponse(_message.Message):
    __slots__ = ("status", "message")
    class Status(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        SUCCESS: _ClassVar[DeleteAgentResponse.Status]
        ERROR: _ClassVar[DeleteAgentResponse.Status]
    SUCCESS: DeleteAgentResponse.Status
    ERROR: DeleteAgentResponse.Status
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    status: DeleteAgentResponse.Status
    message: str
    def __init__(self, status: _Optional[_Union[DeleteAgentResponse.Status, str]] = ..., message: _Optional[str] = ...) -> None: ...
