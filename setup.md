# Setting up your server with a verified role

The verified role is the role that should be given out to all soton students and staff. People without this role should
be able to access as little of the server as possible.

## Permissions

Edit your role permissions by going to server settings and then roles.

Edit the `@everyone` role so that members can't view channels.

![image](https://user-images.githubusercontent.com/49870539/189497744-3d4edd6c-ea09-4da5-af0f-b2d5c290ade1.png)

Create a `verified` role that allows it's members to view channels.

![image](https://user-images.githubusercontent.com/49870539/189497802-bd553d78-2949-4abc-95ab-50a6a532bf76.png)

## Channels

We recommend only having two channels for unverified users.

A read-me channel to welcome users and explain how they should verify themselves. The read-me channel should not allow
users to send messages.

![image](https://user-images.githubusercontent.com/49870539/189498750-a393d7ad-dfbf-41b4-82f2-1ee6ee4be059.png)

And a verify-yourself channel that allows people to send messages so they can interact with the bot. We highly recommend
changing the permissions on the verify-yourself channel so that verified users can't see it. This can be done by going
to edit channel, clicking permissions and removing the view channel permission.

![image](https://user-images.githubusercontent.com/49870539/189498945-bd1cfe64-9725-4b6d-a5a6-41fe9949f61d.png)

## Final Words

We recommend running the /re-verify command to verify all the people who have already verified with the service on
another server.

One important note is that due to how Discord permissions work secret channels that can only be accessed using a certain
role (such as a first year area) can still be accessed even if you are not verified. As such we highly recommend that
role-me channels are inaccessible to unverified people.

You should now have everything setup, to check everything has worked we recommend viewing the server as the verified
role and everyone role. This can be done by going back into the role editing menu and then clicking the 3 dots at the
top left.

Once you are happy with your changes notify your users that they will need to verify themselves by going
to https://sotonverify.link/ and then running the /verify command. 
